// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: alignments laid out and edited, pylons built and placed, routes,
//! circuits, jumpers edited.

use super::*;

/// The New alignment tool: from a point of an alignment, through a free
/// point on the ground, back to another point of it — a new alignment
/// whose ends are junctions. Moving a junction point (Edit) moves both;
/// the Edit tool moves a point to another alignment and place. Escape
/// gives a new alignment up.
#[test]
fn alignments_are_laid_out_joined_and_edited() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project("new-alignment");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let main = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.model.line.alignment_by_id("main").unwrap().points.clone()
    };
    let position = |h: &mut Harness<'_, App>, id: &str| {
        let r = h.state_mut().ready().unwrap();
        r.model.line.alignment_point(id).unwrap().position
    };
    let before = main(&mut harness);
    assert!(before.len() >= 3);
    let p0 = position(&mut harness, &before[0]);
    let free = [p0[0] + 40.0, p0[1] + 60.0, p0[2]];
    let at = |id: &String| Selection::AlignmentPoint { id: id.clone() };
    use_tool(&mut harness, Tool::NewAlignment, at(&before[0]), None);
    // Nothing to snap to: the coordinates are asked (E, N, height).
    use_tool(
        &mut harness,
        Tool::NewAlignment,
        Selection::None,
        Some([free[0], free[1], 0.0]),
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let d = r.coord_dialog.as_mut().expect("the coordinates asked");
        assert_eq!(d.text[0], format!("{:.3}", free[0]));
        d.text[2].clear();
        assert!(d.point().is_err(), "the height is required");
        d.text[2] = format!("{}", free[2]);
        let p = d.point().unwrap();
        r.coord_dialog = None;
        r.place_alignment_point(
            crate::alignment_tools::CoordFor::NewAlignment,
            ohl_line::AlignmentPoint::free("", p),
        );
    }
    use_tool(&mut harness, Tool::NewAlignment, at(&before[2]), None);
    assert_eq!(harness.state_mut().ready().unwrap().view3d.draft.len(), 3);
    // Enter: its route is asked (main: its existing points are main's).
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        let c = r.route_choice.as_ref().expect("the route asked");
        assert_eq!(c.route, "main");
        r.make_new_alignment("main".into(), false);
    }
    harness.run();
    let new = {
        let r = harness.state_mut().ready().unwrap();
        assert!(
            r.new_alignment.is_empty() && r.route_choice.is_none(),
            "{:?}",
            r.status
        );
        let n = r
            .model
            .line
            .alignment_by_id("alignment 1")
            .unwrap_or_else(|| panic!("{:?}", r.status))
            .clone();
        assert_eq!(n.route, "main");
        n.points
    };
    // The existing points shared (junctions), one new point.
    assert_eq!(new.len(), 3);
    assert_eq!(new[0], before[0]);
    assert_eq!(position(&mut harness, &new[1]), free);
    assert_eq!(new[2], before[2]);
    // The junction moved with the Edit tool: one point, on both.
    use_tool(&mut harness, Tool::Edit, at(&before[0]), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Alignment(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(
            f.orders,
            [("main".to_owned(), 1), ("alignment 1".to_owned(), 1)]
        );
        f.structure = None;
        f.position[0] += 2.0;
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
    }
    assert!((position(&mut harness, &before[0])[0] - (p0[0] + 2.0)).abs() < 1e-9);
    // Point 2 of main also on alignment 1, as its second point: it keeps
    // its place on main.
    use_tool(&mut harness, Tool::Edit, at(&before[1]), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Alignment(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        f.orders.push(("alignment 1".into(), 2));
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let a1 = &r.model.line.alignment_by_id("alignment 1").unwrap().points;
        assert_eq!(a1.len(), 4);
        assert_eq!(a1[1], before[1]);
    }
    assert_eq!(main(&mut harness), before);
    // An alignment without structures: its profile is drawn.
    {
        let r = harness.state_mut().ready().unwrap();
        r.profile.show = Some(crate::profile::Show::Alignment("alignment 1".into()));
    }
    harness.state_mut().show_tab(crate::app::Tab::Profile);
    harness.run_steps(3);
    // Escape gives a new alignment up; the tool stays held.
    use_tool(&mut harness, Tool::NewAlignment, at(&before[0]), None);
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.new_alignment.is_empty() && r.view3d.draft.is_empty());
    assert_eq!(r.hotbar.tool(), Tool::NewAlignment);
}

/// Points placed on no alignment (New alignment point tool), then picked
/// one after another into a new alignment; with no route yet, one is
/// offered first.
#[test]
fn free_points_are_picked_into_an_alignment_and_a_route_is_offered() {
    use crate::alignment_tools::{CoordFor, RouteChoice};
    use crate::tools::Tool;
    let model = synthetic_project_with("free-points", |line| {
        line.routes.clear();
        line.alignments.clear();
        line.alignment_points.clear();
        for s in &mut line.structures {
            s.routes.clear();
        }
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // Two free points (Add alignment point, not connected): one under T1
    // (tied, its coordinates given), one typed.
    use_tool(
        &mut harness,
        Tool::AddAlignment,
        Selection::Structure("T1".into()),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let d = r.coord_dialog.as_ref().expect("its window, always");
        assert_eq!(d.purpose, CoordFor::AddAlignment);
        assert_eq!(d.structure.as_deref(), Some("T1"));
    }
    harness.run();
    harness.get_by_label("Add the point").click();
    harness.run();
    use_tool(
        &mut harness,
        Tool::AddAlignment,
        Selection::None,
        Some([10.0, 20.0, 0.0]),
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let d = r.coord_dialog.as_mut().expect("asked");
        d.text[2] = "500".into();
    }
    harness.run();
    harness.get_by_label("Add the point").click();
    harness.run();
    let ids: Vec<String> = {
        let r = harness.state_mut().ready().unwrap();
        let l = &r.model.line;
        assert_eq!(l.alignment_points.len(), 2, "{:?}", r.status);
        assert!(l.alignments.is_empty());
        assert_eq!(l.alignment_points[0].structure.as_deref(), Some("T1"));
        l.alignment_points.iter().map(|p| p.id.clone()).collect()
    };
    // Picked one after another, Enter: no route yet — one is offered.
    for id in &ids {
        use_tool(
            &mut harness,
            Tool::NewAlignment,
            Selection::AlignmentPoint { id: id.clone() },
            None,
        );
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        let c: &RouteChoice = r.route_choice.as_ref().expect("asked");
        assert!(c.route.is_empty());
        assert_eq!(c.new_route, "route 1", "a new route proposed");
    }
    harness.run();
    harness
        .get_by_label("Create the route and the alignment")
        .click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let l = &r.model.line;
    assert_eq!(l.routes.len(), 1, "{:?}", r.status);
    let a = &l.alignments[0];
    assert_eq!(a.route, "route 1");
    assert_eq!(a.points, ids);
    assert_eq!(l.routes_of("T1"), ["route 1"]);
}

/// Edit alignment: a stretch between two points replaced by new points
/// (those between stay, on no alignment); from an end, Enter adds them
/// there; from a point in the middle, Enter offers a new alignment.
#[test]
fn the_edit_alignment_tool_reconnects_extends_and_branches() {
    use crate::tools::Tool;
    let model = synthetic_project_with("reroute", |l| {
        l.alignment_points.clear();
        l.alignments = vec![ohl_line::Alignment::new("main", "main", Vec::new())];
        l.set_alignment_points(
            "main",
            [0.0, 100.0, 200.0, 300.0].map(|x| ([x, -50.0, 500.0], None)),
        );
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let main = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.model.line.alignment_by_id("main").unwrap().points.clone()
    };
    let at = |id: &String| Selection::AlignmentPoint { id: id.clone() };
    let typed = |h: &mut Harness<'_, App>, p: [f64; 3]| {
        let r = h.state_mut().ready().unwrap();
        r.coord_dialog = None;
        r.place_alignment_point(
            crate::alignment_tools::CoordFor::Reroute,
            ohl_line::AlignmentPoint::free("", p),
        );
    };
    let before = main(&mut harness);
    // P1 → a new point → P4: P2 and P3 replaced (they stay).
    use_tool(&mut harness, Tool::EditAlignment, at(&before[0]), None);
    typed(&mut harness, [150.0, -80.0, 500.0]);
    use_tool(&mut harness, Tool::EditAlignment, at(&before[3]), None);
    let after = main(&mut harness);
    assert_eq!(
        after.len(),
        3,
        "{:?}",
        harness.state_mut().ready().unwrap().status
    );
    assert_eq!((&after[0], &after[2]), (&before[0], &before[3]));
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.new_alignment.is_empty() && !r.reroute);
        let l = &r.model.line;
        assert_eq!(
            l.alignment_point(&after[1]).unwrap().position,
            [150.0, -80.0, 500.0]
        );
        assert!(l.alignment_point(&before[1]).is_some(), "kept");
        assert!(l.point_alignments(&before[1]).is_empty());
    }
    // From the last point, Enter: added at the end.
    use_tool(&mut harness, Tool::EditAlignment, at(&before[3]), None);
    typed(&mut harness, [400.0, -50.0, 500.0]);
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(main(&mut harness).len(), 4);
    // From the first point, Enter: added at the start.
    use_tool(&mut harness, Tool::EditAlignment, at(&before[0]), None);
    typed(&mut harness, [-100.0, -50.0, 500.0]);
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let now = main(&mut harness);
    assert_eq!(now.len(), 5);
    assert_eq!(now[1], before[0]);
    // From a point in the middle, Enter: a new alignment is offered (an
    // alignment has two ends only), starting at that point.
    let middle = now[2].clone();
    use_tool(&mut harness, Tool::EditAlignment, at(&middle), None);
    typed(&mut harness, [150.0, 100.0, 500.0]);
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        let c = r.route_choice.as_ref().expect("a new alignment offered");
        assert_eq!(c.points[0].id, middle);
        assert_eq!(c.route, "main");
        r.make_new_alignment("main".into(), false);
        let l = &r.model.line;
        assert_eq!(l.alignments.len(), 2, "{:?}", r.status);
        assert_eq!(l.point_alignments(&middle).len(), 2, "shared");
    }
    assert_eq!(main(&mut harness), now, "main unchanged");
}

/// Tools › New pylon from a template…: a suspension and a strain type
/// built from a template (sets: the slots, + 20 back, + 40 ahead); Add
/// structure on an alignment point, the alignment's other points ticked
/// (what Place on alignment points… did): one command puts them on every
/// point, along the line, a row's own type at the angle point; built
/// again, the structures follow; Put it on the line again turns a
/// structure back.
#[test]
fn pylons_are_built_from_templates_and_placed_on_the_alignment() {
    use crate::editors::Form;
    use crate::pylon_builder::PylonBuilder;
    use crate::tools::Tool;
    use ohl_structures::templates::Role;
    let model = synthetic_project_with("pylon-builder", |l| {
        l.alignment_points.clear();
        l.alignments = vec![ohl_line::Alignment::new("main", "main", Vec::new())];
        l.set_alignment_points(
            "main",
            [[0.0, -80.0], [300.0, -80.0], [600.0, -20.0], [900.0, -20.0]]
                .map(|p| ([p[0], p[1], 500.0], None)),
        );
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // The suspension type, then the strain one (the toggle).
    {
        let r = harness.state_mut().ready().unwrap();
        let mut b = PylonBuilder::new(&r.model);
        let k = (0..10)
            .find(|&k| {
                b.choose(&r.model, k);
                b.current().is_some_and(|t| t.id == "danube-2c")
            })
            .expect("the shipped template");
        b.choose(&r.model, k);
        assert_eq!(b.id, "danube-2c-susp");
        r.pylon_builder = Some(b);
    }
    harness.run();
    harness.get_by_label("Save the type").click();
    harness.run();
    harness.get_by_label("Strain").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let b = r.pylon_builder.as_mut().unwrap();
        assert_eq!(b.role, Role::Strain);
        b.rebuild = None;
        b.id = "danube-2c-strain".into();
    }
    harness.run();
    harness.get_by_label("Save the type").click();
    harness.run();
    let sets = |h: &mut Harness<'_, App>, id: &str| {
        let r = h.state_mut().ready().unwrap();
        let types = &r.model.types.as_ref().expect("types read").0;
        types
            .get(id)
            .unwrap_or_else(|| panic!("{id}: {:?}", r.pylon_builder.as_ref().unwrap().message))
            .sets()
    };
    assert_eq!(sets(&mut harness, "danube-2c-susp"), [1, 2, 3, 4, 5, 6, 19]);
    assert_eq!(
        sets(&mut harness, "danube-2c-strain"),
        [21, 22, 23, 24, 25, 26, 39, 41, 42, 43, 44, 45, 46, 59]
    );
    // On every point: suspension, the angle point strain.
    let before = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .structures
        .len();
    // Add structure on the alignment's first point; its other points
    // ticked in the window (All), the angle point its own type.
    let first = {
        let r = harness.state_mut().ready().unwrap();
        r.pylon_builder = None;
        let l = &r.model.line;
        l.points_of(l.alignment_by_id("main").unwrap())[0]
            .id
            .clone()
    };
    use_tool(
        &mut harness,
        Tool::AddStructure,
        Selection::AlignmentPoint { id: first },
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Structure(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.on_alignment, Some(("main".into(), 0)), "on the point");
        f.structure_type = Some("danube-2c-susp".into());
    }
    harness.run();
    harness
        .get_by_label("Also on other points of this alignment (0 ticked)")
        .click();
    harness.run();
    harness.get_by_label("All").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Structure(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        let more = f.more.as_mut().expect("the other points");
        assert_eq!(
            more.rows.iter().map(|r| r.index).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        more.rows[1].structure_type = Some("danube-2c-strain".into());
    }
    harness.run();
    harness.get_by_label("Also on other points of this alignment (3 ticked)");
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    harness.run();
    let placed: Vec<ohl_line::Structure> = {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let l = &r.model.line;
        assert_eq!(l.structures.len(), before + 4);
        let pts = l.points_of(l.alignment_by_id("main").unwrap());
        pts.iter()
            .map(|p| {
                l.structure(p.structure.as_deref().expect("tied"))
                    .unwrap()
                    .clone()
            })
            .collect()
    };
    assert_eq!(
        placed[2].structure_type.as_deref(),
        Some("danube-2c-strain")
    );
    assert_eq!(placed[1].structure_type.as_deref(), Some("danube-2c-susp"));
    // Along the line: the first one's right is south (heading east).
    assert!(
        (placed[0].orientation_deg + 90.0).abs() < 1e-6,
        "{}",
        placed[0].orientation_deg
    );
    // One command: one undo takes all four.
    {
        let r = harness.state_mut().ready().unwrap();
        r.perform(crate::edit::Action::Undo);
        assert_eq!(r.model.line.structures.len(), before);
        r.perform(crate::edit::Action::Redo);
        assert_eq!(r.model.line.structures.len(), before + 4);
    }
    // Built again with a higher lower crossarm: its structures follow.
    let z = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        let s = r.model.line.structure(&placed[1].id).unwrap().clone();
        let ty = r
            .model
            .types
            .as_ref()
            .unwrap()
            .0
            .get("danube-2c-susp")
            .unwrap();
        ty.point_local("A-C1-L2", None).unwrap()[2] + s.position[2]
    };
    let low = z(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        let mut b = PylonBuilder::new(&r.model);
        b.rebuild(&r.model, "danube-2c-susp").unwrap();
        *b.values.dimensions.get_mut("lower_level").unwrap() += 2.0;
        r.pylon_builder = Some(b);
    }
    harness.run();
    harness
        .get_by_label("Build it again (keep what was added)")
        .click();
    harness.run();
    assert!((z(&mut harness) - (low + 2.0)).abs() < 1e-6);
    // Turned by hand, then put on the line again.
    {
        let r = harness.state_mut().ready().unwrap();
        let mut f = crate::editors::StructureForm::of(&r.model, &placed[0].id).unwrap();
        assert!(f.on_alignment.is_some(), "it knows its point");
        f.orientation_deg += 30.0;
        assert!(f.put_on_line(&r.model));
        assert!((f.orientation_deg - placed[0].orientation_deg).abs() < 1e-6);
    }
}

/// Add structure with no structure type in the library: the window opens
/// (also with no ground under the click), says so and offers New pylon
/// from a template.
#[test]
fn add_structure_without_types_offers_a_new_pylon() {
    use crate::tools::Tool;
    let model = synthetic_project_with("no-types", |l| {
        l.structures.clear();
        l.sections.clear();
        l.jumpers.clear();
        for p in &mut l.alignment_points {
            p.structure = None;
        }
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // No ground under the click: it opens anyway.
    use_tool(&mut harness, Tool::AddStructure, Selection::None, None);
    assert!(harness.state_mut().ready().unwrap().editor.is_some());
    harness.run();
    harness.get_by_label_contains("No structure type in the library yet");
    harness
        .query_all_by_label("New structure from a template…")
        .last()
        .unwrap()
        .click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.pylon_builder.is_some() && r.editor.is_some(), "both open");
}

/// A selected structure's wire points are drawn where its strings hold
/// them in the case shown (a suspension string swung by the wind); one no
/// wire is strung to stays where the structure gives it, in still air.
#[test]
fn selected_wire_points_follow_their_strings() {
    let model = solved_all(synthetic());
    let line = &model.line;
    // A case with wind: the suspension strings swing.
    let windy = model
        .cases
        .iter()
        .position(|c| c.case.wind_pressure.si() > 0.0)
        .expect("a case with wind");
    let pieces = crate::selection::string_pieces(&model, windy);
    let strung: std::collections::BTreeSet<(String, u32)> = line
        .sections
        .iter()
        .flat_map(|s| s.points.iter().map(|p| (p.structure.clone(), p.set)))
        .collect();
    let mut moved = 0;
    for st in &line.structures {
        for w in st.wires.iter().filter(|w| w.set.is_some()) {
            let drawn = crate::scene::wire_drawn(st, w, &pieces);
            let held = st.strings.iter().any(|x| x.wire == w.label);
            if !held || !strung.contains(&(st.id.clone(), w.set.unwrap())) {
                assert_eq!(drawn, [w.position], "{} {}", st.id, w.label);
            } else if drawn.iter().any(|p| {
                (0..3)
                    .map(|k| (p[k] - w.position[k]).powi(2))
                    .sum::<f64>()
                    .sqrt()
                    > 0.05
            }) {
                moved += 1;
            }
        }
    }
    assert!(
        moved > 0,
        "some wire point drawn where its string swings to"
    );
    // A wire point no string reaches, not strung: as given.
    let mut st = line.structures[0].clone();
    st.wires.push(ohl_line::WirePoint {
        description: String::new(),
        label: "spare".into(),
        position: [1.0, 2.0, 3.0],
        set: Some(77),
        weight: 0.0,
    });
    let w = st.wires.last().unwrap().clone();
    assert_eq!(
        crate::scene::wire_drawn(&st, &w, &pieces),
        [[1.0, 2.0, 3.0]]
    );
}

/// The three tables: Routes, Alignments (each on one route, in order) and
/// Alignment points (a row per point and alignment, with its place there).
#[test]
fn routes_alignments_and_points_are_edited_in_their_tables() {
    use crate::tables::TableKind;
    let model = synthetic_project("table-new-alignment");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // A new alignment needs its route.
    {
        let r = harness.state_mut().ready().unwrap();
        let mut row = TableKind::Alignments.new_row(&r.model, 0, &[]);
        assert_eq!(row[0], "alignment 1");
        row[0] = "spur".into();
        row[2] = String::new();
        r.tables
            .entry(TableKind::Alignments)
            .or_default()
            .added
            .push(row);
    }
    let e = apply_table(&mut harness, TableKind::Alignments, false).unwrap_err();
    assert!(e.contains("choose its route"), "{e}");
    {
        let r = harness.state_mut().ready().unwrap();
        let page = r.tables.get_mut(&TableKind::Alignments).unwrap();
        page.added[0][2] = "branch".into();
        // Between main and the branch.
        page.added[0][3] = "1.5".into();
    }
    apply_table(&mut harness, TableKind::Alignments, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let ids: Vec<&str> = r
            .model
            .line
            .alignments
            .iter()
            .map(|a| a.id.as_str())
            .collect();
        assert_eq!(ids, ["main", "spur", "branch"]);
        assert_eq!(
            r.model.line.alignment_by_id("spur").unwrap().route,
            "branch"
        );
    }
    // Reordered: the branch first.
    set_cell(&mut harness, TableKind::Alignments, "branch", "#", "0.5");
    apply_table(&mut harness, TableKind::Alignments, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let ids: Vec<&str> = r
            .model
            .line
            .alignments
            .iter()
            .map(|a| a.id.as_str())
            .collect();
        assert_eq!(ids, ["branch", "main", "spur"]);
    }
    // Points: main's third point on the spur too (shared), and a new one.
    let (third, rows_before) = {
        let r = harness.state_mut().ready().unwrap();
        let third = r.model.line.alignment_by_id("main").unwrap().points[2].clone();
        let mut a = TableKind::AlignmentPoints.new_row(&r.model, 0, &[]);
        a[0] = "spur".into();
        a[1] = "1".into();
        a[2] = third.clone();
        let mut b = TableKind::AlignmentPoints.new_row(&r.model, 0, &[a.clone()]);
        b[0] = "spur".into();
        b[1] = "2".into();
        b[3] = "1300".into();
        b[4] = "150".into();
        b[5] = "505".into();
        let n = TableKind::AlignmentPoints.rows(&r.model, 0).len();
        let page = r.tables.entry(TableKind::AlignmentPoints).or_default();
        page.added.push(a);
        page.added.push(b);
        (third, n)
    };
    apply_table(&mut harness, TableKind::AlignmentPoints, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let l = &r.model.line;
        let spur = &l.alignment_by_id("spur").unwrap().points;
        assert_eq!(spur.len(), 2);
        assert_eq!(spur[0], third);
        assert_eq!(
            l.alignment_point(&spur[1]).unwrap().position,
            [1300.0, 150.0, 505.0]
        );
        // The shared point: a row on each alignment, each with its place.
        let rows = TableKind::AlignmentPoints.rows(&r.model, 0);
        assert_eq!(rows.len(), rows_before + 2);
        let on_spur = rows
            .iter()
            .find(|x| x.key == format!("spur\t{third}"))
            .unwrap();
        assert_eq!(on_spur.cells[1], "1");
        assert_eq!(on_spur.cells[7], "main #3");
    }
    // Taken off main (its alignment emptied): it stays on the spur.
    set_cell(
        &mut harness,
        TableKind::AlignmentPoints,
        &format!("main\t{third}"),
        "alignment",
        "",
    );
    apply_table(&mut harness, TableKind::AlignmentPoints, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.line.point_alignments(&third), ["spur"]);
    }
    // A row deleted: the point off every alignment (asked when shared).
    {
        let r = harness.state_mut().ready().unwrap();
        r.tables
            .entry(TableKind::AlignmentPoints)
            .or_default()
            .deleted
            .insert(format!("spur\t{third}"));
    }
    apply_table(&mut harness, TableKind::AlignmentPoints, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.alignment_point(&third).is_none());
    assert_eq!(
        r.model.line.alignment_by_id("spur").unwrap().points.len(),
        1
    );
}

/// Sections › Electrical circuits: one row per conductor path, counted
/// from the connections; each end is a start or an end — ticked in
/// "starts" (several feeds possible), swapped by ⇄ — at least one of each;
/// its jumpers point along it; back to the line's direction, nothing kept.
#[test]
fn circuits_are_listed_and_turned() {
    use crate::tables::TableKind;
    let model = synthetic_project("circuits");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let row = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        let rows = TableKind::ConductorPaths.rows(&r.model, 0);
        rows.into_iter()
            .find(|x| x.cells[6].contains("S3"))
            .unwrap()
    };
    let (key, start, ends) = {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::ConductorPaths.rows(&r.model, 0);
        assert_eq!(
            rows.len(),
            ohl_line::electrical::conductor_paths(&r.model.line).len()
        );
        assert!(TableKind::ConductorPaths.fixed_rows());
        let x = rows.iter().find(|x| x.cells[6].contains("S3")).unwrap();
        assert!(x.cells[4].starts_with("T1 "), "{}", x.cells[4]);
        assert_eq!(x.cells[5].split(';').count(), 2, "two ends: {}", x.cells[5]);
        (x.key.clone(), x.cells[4].clone(), x.cells[5].clone())
    };
    // Reversed (⇄ puts its ends in "starts"): two starts, one end.
    set_cell(
        &mut harness,
        TableKind::ConductorPaths,
        &key,
        "starts",
        &ends,
    );
    apply_table(&mut harness, TableKind::ConductorPaths, false).unwrap();
    {
        let x = row(&mut harness);
        assert_eq!(x.cells[5], start);
        let r = harness.state_mut().ready().unwrap();
        let line = &r.model.line;
        assert_eq!(line.circuit_starts.len(), 2);
        let c = ohl_line::electrical::circuits(line)
            .into_iter()
            .find(|c| c.sections.contains(&"S3".to_owned()))
            .unwrap();
        let at = |p| c.order.iter().position(|q| q == p);
        assert!(
            at(&line.jumpers[0].from) < at(&line.jumpers[0].to),
            "along it"
        );
    }
    // Every end a start: refused.
    let all = format!("{start}; {ends}");
    set_cell(
        &mut harness,
        TableKind::ConductorPaths,
        &key,
        "starts",
        &all,
    );
    let err = apply_table(&mut harness, TableKind::ConductorPaths, false).unwrap_err();
    assert!(err.contains("at least one end"), "{err}");
    harness
        .state_mut()
        .ready()
        .unwrap()
        .tables
        .remove(&TableKind::ConductorPaths);
    // Back along the line: nothing kept.
    set_cell(
        &mut harness,
        TableKind::ConductorPaths,
        &key,
        "starts",
        &start,
    );
    apply_table(&mut harness, TableKind::ConductorPaths, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.circuit_starts.is_empty());
}

/// The Circuits table (2026-10-21): the example's names read into
/// circuits; renaming (the sections follow), adding an empty one (noted),
/// moving a conductor path between circuits, deleting (confirmed) — each
/// Apply one command, undone at once. Refused: a path still in another
/// circuit, another circuit's name (no merge), two paths of one phase not
/// connected (from the table and from Properties' Assign to circuit).
#[test]
fn circuits_are_defined_in_their_table() {
    use crate::tables::TableKind;
    // The branch untapped and of the system: S1 L1, S2 L2, S3 L1.
    let model = synthetic_project_with("circuit-list", |l| {
        l.jumpers.clear();
        l.sections[2].system = Some("ex220".into());
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let rows = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        TableKind::Circuits.rows(&r.model, 0)
    };
    let names = |h: &mut Harness<'_, App>| -> Vec<String> {
        let r = h.state_mut().ready().unwrap();
        r.model
            .line
            .sections
            .iter()
            .map(|s| s.circuit_name.clone())
            .collect()
    };
    let col = |n: &str| TableKind::Circuits.column(n);
    {
        let rows = rows(&mut harness);
        let cells: Vec<(&str, &str, &str)> = rows
            .iter()
            .map(|r| {
                (
                    r.cells[0].as_str(),
                    r.cells[col("members")].as_str(),
                    r.cells[col("notes")].as_str(),
                )
            })
            .collect();
        assert_eq!(
            cells,
            [("Example 1", "S1 S2", ""), ("Branch", "S3", "")],
            "migrated: one circuit per name, the same members"
        );
    }
    // Renamed: its sections follow; one undo takes it back.
    set_cell(
        &mut harness,
        TableKind::Circuits,
        "Example 1",
        "name",
        "North",
    );
    apply_table(&mut harness, TableKind::Circuits, false).unwrap();
    assert_eq!(names(&mut harness), ["North", "North", "Branch"]);
    harness.state_mut().ready().unwrap().model.undo().unwrap();
    assert_eq!(names(&mut harness), ["Example 1", "Example 1", "Branch"]);
    // Another circuit's name: refused (circuits are not merged).
    set_cell(
        &mut harness,
        TableKind::Circuits,
        "Branch",
        "name",
        "Example 1",
    );
    let err = apply_table(&mut harness, TableKind::Circuits, false).unwrap_err();
    assert!(err.contains("not merged"), "{err}");
    harness
        .state_mut()
        .ready()
        .unwrap()
        .tables
        .remove(&TableKind::Circuits);
    // Added, empty: allowed, noted.
    {
        let r = harness.state_mut().ready().unwrap();
        let mut row = TableKind::Circuits.new_row(&r.model, 0, &[]);
        assert_eq!(row[0], "C1");
        row[1] = "South".into();
        row[col("description")] = "invented".into();
        r.tables
            .entry(TableKind::Circuits)
            .or_default()
            .added
            .push(row);
    }
    apply_table(&mut harness, TableKind::Circuits, false).unwrap();
    {
        let rows = rows(&mut harness);
        let south = rows.iter().find(|r| r.key == "C1").unwrap();
        assert!(south.cells[col("notes")].contains("no conductor path"));
        assert_eq!(south.cells[col("description")], "invented");
    }
    // S3 into South while still in Branch: refused.
    set_cell(&mut harness, TableKind::Circuits, "C1", "members", "S3");
    let err = apply_table(&mut harness, TableKind::Circuits, false).unwrap_err();
    assert!(err.contains("is in circuit \"Branch\""), "{err}");
    // Taken out of Branch in the same Apply: moved, one command.
    set_cell(&mut harness, TableKind::Circuits, "Branch", "members", "");
    apply_table(&mut harness, TableKind::Circuits, false).unwrap();
    assert_eq!(names(&mut harness), ["Example 1", "Example 1", "South"]);
    harness.state_mut().ready().unwrap().model.undo().unwrap();
    assert_eq!(names(&mut harness), ["Example 1", "Example 1", "Branch"]);
    // S3 (L1) into Example 1, whose L1 is S1, not connected: refused.
    set_cell(&mut harness, TableKind::Circuits, "Branch", "members", "");
    set_cell(
        &mut harness,
        TableKind::Circuits,
        "Example 1",
        "members",
        "S1 S2 S3",
    );
    let err = apply_table(&mut harness, TableKind::Circuits, false).unwrap_err();
    assert!(err.contains("not connected"), "{err}");
    harness
        .state_mut()
        .ready()
        .unwrap()
        .tables
        .remove(&TableKind::Circuits);
    // The same from Properties: refused; into South (empty): moved.
    {
        let r = harness.state_mut().ready().unwrap();
        let err = crate::properties::assign_circuit_action(&r.model, "S3", Some("Example 1"))
            .err()
            .unwrap();
        assert!(err.contains("not connected"), "{err}");
        let a = crate::properties::assign_circuit_action(&r.model, "S3", Some("C1")).unwrap();
        r.perform(a);
    }
    assert_eq!(names(&mut harness), ["Example 1", "Example 1", "South"]);
    // Deleted: its paths in no circuit; listed to confirm first.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .tables
        .entry(TableKind::Circuits)
        .or_default()
        .deleted
        .insert("C1".into());
    assert!(apply_table(&mut harness, TableKind::Circuits, false).is_err());
    apply_table(&mut harness, TableKind::Circuits, true).unwrap();
    assert_eq!(names(&mut harness), ["Example 1", "Example 1", ""]);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.circuit("C1").is_none());
    r.model.undo().unwrap();
    assert!(r.model.line.circuit("C1").is_some());
    assert_eq!(r.model.line.sections[2].circuit_name, "South");
}

/// The Edit tool on a jumper opens its window, as on a span (the mouse is
/// freed); Apply changes the jumper in one command.
#[test]
fn jumpers_are_edited_in_their_window() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project("jumper-window");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().ready().unwrap().view3d.captured = true;
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::Jumper("J-T3-branch".into()),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Jumper(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert!(!r.view3d.captured, "the window frees the mouse");
        assert_eq!(f.jumper.sag, Some(1.5));
        f.jumper.sag = Some(2.0);
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        assert_eq!(r.model.line.jumpers[0].sag, Some(2.0));
    }
    // Rigid: no length nor sag.
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::Jumper("J-T3-branch".into()),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Jumper(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        f.jumper.rigid = true;
        f.jumper.sag = None;
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.editor.is_none(), "{:?}", r.status);
    assert!(r.model.line.jumpers[0].rigid && r.model.line.jumpers[0].sag.is_none());
}

/// A section's phase, circuit name or system changed — in its editor, in
/// the Sections table, or for the circuit in the Electrical circuits table
/// — goes on all the sections of its circuit, listed first to confirm.
#[test]
fn a_circuit_keeps_one_name_and_phase() {
    use crate::editors::Form;
    use crate::tables::TableKind;
    use crate::tools::Tool;
    let model = synthetic_project("circuit-phase");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let phase = |h: &mut Harness<'_, App>, id: &str| {
        let r = h.state_mut().ready().unwrap();
        let s = r.model.line.sections.iter().find(|s| s.id == id).unwrap();
        (s.phase_label.clone(), s.circuit_name.clone())
    };
    // S1 and the branch S3 are one circuit (the jumper at T3).
    // The editor: the first Apply shows S3, the second applies.
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::Span {
            section: "S1".into(),
            span: 0,
        },
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Section(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        f.phase_label = "L3".into();
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_some(), "not applied yet");
        let warned = r.circuit_warned.clone().unwrap();
        assert_eq!(warned, vec![("S3".to_owned(), vec!["phase"])]);
    }
    assert_eq!(phase(&mut harness, "S3").0, "L1");
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        assert!(r.circuit_warned.is_none());
    }
    assert_eq!(phase(&mut harness, "S1").0, "L3");
    assert_eq!(phase(&mut harness, "S3").0, "L3");
    // The Sections table: confirmed first, then both.
    set_cell(&mut harness, TableKind::Sections, "S3", "phase", "L2");
    assert_eq!(
        apply_table(&mut harness, TableKind::Sections, false).unwrap_err(),
        "needs a confirmation"
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let confirm = r.tables[&TableKind::Sections].confirm.clone().unwrap();
        assert!(
            confirm.iter().any(|t| t.contains("S1 (phase)")),
            "{confirm:?}"
        );
    }
    apply_table(&mut harness, TableKind::Sections, true).unwrap();
    assert_eq!(phase(&mut harness, "S1").0, "L2");
    assert_eq!(phase(&mut harness, "S3").0, "L2");
    // The circuits table: its names differ ("mixed"); one name for all.
    let key = {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::ConductorPaths.rows(&r.model, 0);
        let row = rows.iter().find(|x| x.cells[6].contains("S3")).unwrap();
        assert!(row.cells[1].starts_with("mixed:"), "{}", row.cells[1]);
        assert_eq!(row.cells[2], "L2");
        row.key.clone()
    };
    set_cell(
        &mut harness,
        TableKind::ConductorPaths,
        &key,
        "circuit",
        "Main",
    );
    assert!(apply_table(&mut harness, TableKind::ConductorPaths, false).is_err());
    apply_table(&mut harness, TableKind::ConductorPaths, true).unwrap();
    assert_eq!(phase(&mut harness, "S1").1, "Main");
    assert_eq!(phase(&mut harness, "S3").1, "Main");
}

/// A project whose library has a crafted name ("../…": written by another
/// program, or a crafted copy): exporting the libraries refuses it, and
/// nothing is written outside the folder chosen.
#[test]
fn exported_libraries_stay_in_their_folder() {
    let mut model = synthetic_project("export-names");
    let file = model.sources.file.clone();
    // Put as an object directly (the session's own checks bypassed).
    model
        .editing
        .as_mut()
        .unwrap()
        .session
        .apply(
            "tester",
            ohl_session::Command::Put {
                kind: ohl_session::Kind::Library,
                id: "cables/../escaped.ohlw".into(),
                value: serde_json::json!({
                    "id": "cables/../escaped.ohlw",
                    "kind": "cables",
                    "name": "../escaped.ohlw",
                    "text": "x"
                }),
                ord: None,
            },
        )
        .unwrap();
    let dir = file.parent().unwrap().join("export");
    let err = model.export_libraries(&dir).unwrap_err();
    assert!(err.contains("not a plain file name"), "{err}");
    assert!(
        !dir.join("escaped.ohlw").exists() && !dir.parent().unwrap().join("escaped.ohlw").exists()
    );
}
