// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: an empty project's first alignment, structures placed and
//! edited, the menus, the profile, routes.

use super::*;

/// An empty project: the 3D view shows the ground; the first alignment is
/// the line's own (it was refused: an empty "main" beside it); the view
/// fits to the first structure.
#[test]
fn an_empty_project_gets_its_first_alignment_and_a_view() {
    use crate::tools::Tool;
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-empty", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
    std::fs::remove_file(&path).ok();
    let mut line = ohl_line::Line::load(&ex.join("synthetic-line.ohl.json")).unwrap();
    line.structures.clear();
    line.sections.clear();
    line.jumpers.clear();
    line.routes.clear();
    line.alignments.clear();
    line.alignment_points.clear();
    line.systems.clear();
    ohl_session::Session::open(&path)
        .unwrap()
        .import(&line, "tester")
        .unwrap();
    let model = model::load(
        Sources {
            file: path,
            library: Some(root().join("libraries/cables")),
            terrain: Some(ex.join("synthetic-survey.csv")),
            point_budget: 1000,
            corridor: 60.0,
            user: Some("tester".into()),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    // The 3D chunks are built within a time budget per frame: a busy
    // machine needs more frames than the default.
    let mut builder = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .with_max_steps(64);
    if gpu {
        builder = builder.wgpu();
    }
    let mut harness =
        builder.build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    if gpu {
        // Fitted to the ground, not to a point.
        let cam = harness.state_mut().ready().unwrap().view3d.camera.unwrap();
        assert!(cam.distance > 100.0, "{}", cam.distance);
    }
    let ground = |h: &mut Harness<'_, App>, x: f64, y: f64| {
        let r = h.state_mut().ready().unwrap();
        [x, y, r.model.ground_at(x, y).unwrap()]
    };
    // Step 1: the route (the window of the first steps).
    assert!(harness.state_mut().ready().unwrap().first_steps.is_some());
    harness.get_by_label("Create the route").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.first_steps.is_none(), "{:?}", r.status);
        assert_eq!(r.model.line.routes[0].id, "route 1");
        // Step 2: the New alignment tool held.
        assert_eq!(r.hotbar.tool(), Tool::NewAlignment);
    }
    for (x, y) in [(0.0, 0.0), (310.0, 0.0), (640.0, 560.0)] {
        let p = ground(&mut harness, x, y);
        // Nothing to snap to: the coordinates window, confirmed.
        use_tool(&mut harness, Tool::NewAlignment, Selection::None, Some(p));
        let r = harness.state_mut().ready().unwrap();
        let point = r.coord_dialog.take().expect("asked").point().unwrap();
        r.place_alignment_point(
            crate::alignment_tools::CoordFor::NewAlignment,
            ohl_line::AlignmentPoint::free("", point),
        );
    }
    // Enter: its route, the only one, chosen.
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.route_choice.as_ref().unwrap().route, "route 1");
        r.make_new_alignment("route 1".into(), false);
        let line = &r.model.line;
        assert_eq!(line.alignments.len(), 1, "{:?}", r.status);
        assert_eq!(line.alignments[0].points.len(), 3);
        assert_eq!(line.alignments[0].route, "route 1");
        assert_eq!(line.routes[0].id, "route 1");
    }
    let p = ground(&mut harness, 310.0, 0.0);
    use_tool(&mut harness, Tool::AddStructure, Selection::None, Some(p));
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    harness.run();
    if gpu {
        let cam = harness.state_mut().ready().unwrap().view3d.camera.unwrap();
        assert!(
            (cam.target[0] - 310.0).abs() < 1.0 && cam.target[1].abs() < 1.0,
            "{:?}",
            cam.target
        );
    }
}

/// A new structure on an alignment point: it stands there, tied to it,
/// ahead along the alignment and its cross-arms on the bisector of the
/// line angle; its own points and description kept. The project's angle
/// ahead turns the compass (270: right 0°, ahead 270°). Image with
/// OHL_RENDER_PNG.
#[test]
fn a_structure_is_put_on_an_alignment_point_along_the_line() {
    use crate::editors::{Form, PointRow, StructureForm, compass_direction, line_orientation};
    let gpu = ohl_render::headless_device().is_ok();
    let model = synthetic_project_with("on-alignment", |l| {
        // An alignment of its own: 0;0 → 300;0 → 300;300 (a right angle
        // at its middle point).
        l.routes = vec![ohl_line::Route::new("main", "main")];
        l.alignment_points.clear();
        l.alignments = vec![ohl_line::Alignment::new("main", "main", Vec::new())];
        l.set_alignment_points(
            "main",
            [
                [0.0, 0.0, 500.0],
                [300.0, 0.0, 500.0],
                [300.0, 300.0, 500.0],
            ]
            .map(|p| (p, None)),
        );
    });
    // The bisector at the corner: ahead to the north-east, the right to
    // the south-east (−45° from east); at the ends, across their stretch.
    let line = &model.line;
    let mid = line_orientation(line, [300.0, 0.0, 500.0], Some(&("main".into(), 1))).unwrap();
    assert!((mid - -45.0).abs() < 1e-9, "{mid}");
    let first = line_orientation(line, [0.0, 0.0, 500.0], Some(&("main".into(), 0))).unwrap();
    assert!((first - -90.0).abs() < 1e-9, "{first}");
    // The angle convention: ahead at 270 → 0° is the right (+x).
    let d = compass_direction(0.0, 270.0);
    assert!((d[0] - 1.0).abs() < 1e-12 && d[1].abs() < 1e-12);
    let d = compass_direction(270.0, 270.0);
    assert!(d[0].abs() < 1e-12 && (d[1] - 1.0).abs() < 1e-12);
    let mut builder = Harness::builder().with_size(egui::vec2(1400.0, 950.0));
    if gpu {
        builder = builder.wgpu();
    }
    let mut harness =
        builder.build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.angles_window = Some(270.0);
    }
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .model
            .line
            .display
            .ahead_deg,
        270.0
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let mut f = StructureForm::new_at(&r.model, [301.0, 2.0, 500.0]);
        assert_eq!(
            f.on_alignment,
            Some(("main".into(), 1)),
            "the point near the click"
        );
        f.structure_type = None;
        f.description = "angle tower, invented".into();
        f.points = vec![
            PointRow {
                description: Default::default(),
                label: "A1".into(),
                at: [-6.0, 0.0, 25.0],
            },
            PointRow {
                description: Default::default(),
                label: "A2".into(),
                at: [6.0, 0.0, 25.0],
            },
        ];
        r.editor = Some(Form::Structure(f));
    }
    harness.run();
    harness.run();
    harness.get_by_label("on an alignment point");
    if gpu && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-structure-window.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.editor.is_none(), "{:?}", r.status);
    let id = r.model.line.structures.last().unwrap().id.clone();
    let s = r.model.line.structure(&id).unwrap();
    assert_eq!((s.position[0], s.position[1]), (300.0, 0.0));
    assert!((s.orientation_deg - -45.0).abs() < 1e-9);
    assert_eq!(s.description, "angle tower, invented");
    assert_eq!(s.points.len(), 2);
    let line = &r.model.line;
    assert_eq!(
        line.points_of(&line.alignments[0])[1].structure.as_deref(),
        Some(id.as_str())
    );
    // On the alignment's route, by itself.
    assert_eq!(r.model.line.routes_of(&id), ["main"]);
}

/// Descriptions: typed in the tables, kept, shown as labels with the
/// objects' labels when asked for (Structures › Display options).
#[test]
fn descriptions_are_typed_and_shown_as_labels() {
    use crate::annotations::{Kind, View, object_notes};
    use crate::tables::TableKind;
    let model = synthetic_project("descriptions");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    set_cell(
        &mut harness,
        TableKind::Structures,
        "T2",
        "description",
        "angle tower",
    );
    apply_table(&mut harness, TableKind::Structures, true).unwrap();
    set_cell(
        &mut harness,
        TableKind::Strings,
        "T2\tS1",
        "description",
        "glass, 2019",
    );
    apply_table(&mut harness, TableKind::Strings, true).unwrap();
    set_cell(
        &mut harness,
        TableKind::Sections,
        "S1",
        "description",
        "phase L1",
    );
    apply_table(&mut harness, TableKind::Sections, true).unwrap();
    set_cell(
        &mut harness,
        TableKind::Jumpers,
        "J-T3-branch",
        "description",
        "branch loop",
    );
    apply_table(&mut harness, TableKind::Jumpers, true).unwrap();
    set_cell(
        &mut harness,
        TableKind::Systems,
        "ex220",
        "description",
        "220 kV",
    );
    apply_table(&mut harness, TableKind::Systems, true).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let line = &r.model.line;
    let t2 = line.structure("T2").unwrap();
    assert_eq!(t2.description, "angle tower");
    assert_eq!(t2.string("S1").unwrap().description, "glass, 2019");
    assert_eq!(line.sections[0].description, "phase L1");
    assert_eq!(line.jumpers[0].description, "branch loop");
    assert_eq!(line.systems[0].description, "220 kV");
    // Labels: none until asked for; then the structures, points, strings,
    // wire points, with their descriptions when ticked.
    assert!(object_notes(&r.model, 0, View::Plan, None).is_empty());
    let d = &mut r.model.section_display;
    for ticks in [
        &mut d.structure_label,
        &mut d.point_label,
        &mut d.string_label,
        &mut d.wire_label,
        &mut d.descriptions,
    ] {
        ticks[View::Plan as usize] = true;
    }
    let notes = object_notes(&r.model, 0, View::Plan, None);
    assert!(
        notes
            .iter()
            .all(|n| n.kind == Kind::Object || n.kind == Kind::Structure)
    );
    assert!(notes.iter().any(|n| n.kind == Kind::Structure));
    let texts: Vec<&str> = notes.iter().map(|n| n.text.as_str()).collect();
    assert!(texts.contains(&"T2 — angle tower"), "{texts:?}");
    assert!(texts.contains(&"S1 — glass, 2019"), "{texts:?}");
    assert!(texts.iter().any(|t| t.ends_with("(set 1)")), "{texts:?}");
    // A string's label sits where it is drawn: at its middle as solved.
    let pieces = crate::selection::string_pieces(&r.model, 0);
    let (a, b) = pieces[&("T2".to_owned(), "S1".to_owned())][0];
    let n = notes.iter().find(|n| n.text.starts_with("S1 —")).unwrap();
    assert!(
        (0..3).all(|k| (n.at[k] - 0.5 * (a[k] + b[k])).abs() < 1e-9)
            || notes.iter().filter(|n| n.text.starts_with("S1 —")).count() > 1
    );
    // The insulator's top on its own tick: apart from the structure point
    // it hangs from (a little down the string).
    assert!(!texts.iter().any(|t| t.contains(" top ➡ ")), "{texts:?}");
    r.model.section_display.insulator_label[View::Plan as usize] = true;
    let notes = object_notes(&r.model, 0, View::Plan, None);
    let top = &t2.string("S1").unwrap().top;
    let n = notes
        .iter()
        .find(|n| n.text == format!("S1 top ➡ {top}"))
        .expect("the top's label");
    let p = notes
        .iter()
        .find(|m| m.text.starts_with(top.as_str()))
        .unwrap();
    assert!((0..3).any(|k| (n.at[k] - p.at[k]).abs() > 1e-6));
    // The library labels on their own tick.
    assert!(!notes.iter().any(|n| n.text.starts_with('[')));
    let labelled = r
        .model
        .line
        .structures
        .iter()
        .flat_map(|s| &s.strings)
        .find_map(|x| x.insulator.clone());
    r.model.section_display.library_label[View::Plan as usize] = true;
    let notes = object_notes(&r.model, 0, View::Plan, None);
    if let Some(id) = labelled {
        assert!(notes.iter().any(|n| n.text == format!("[{id}]")));
    }
}

/// A structure type file opened on its own: its editor is the whole
/// window (no line), the pylon in 3D with the compass, its level; the
/// angle ahead is the user's for files opened alone. Image with
/// OHL_RENDER_PNG.
#[test]
fn a_structure_type_opens_on_its_own() {
    let gpu = ohl_render::headless_device().is_ok();
    let file = root().join("examples/synthetic-line/Structures/type-02.ohls");
    let mut builder = Harness::builder().with_size(egui::vec2(1400.0, 950.0));
    if gpu {
        builder = builder.wgpu();
    }
    let mut harness = builder.build_eframe(|cc| {
        let mut app = App::empty(Sources::default(), cc.wgpu_render_state.clone());
        app.ahead_alone = 270.0;
        app.start(
            Sources {
                file: file.clone(),
                ..Sources::default()
            },
            &cc.egui_ctx,
        );
        app
    });
    harness.run();
    harness.run();
    harness.get_by_label("Structure type, opened on its own");
    harness.get_by_label("Level 3 — full design");
    harness.get_by_label("opened alone (no line)");
    assert_eq!(
        harness.state_mut().library_ahead_for_tests(),
        Some(270.0),
        "the compass counts as the user does"
    );
    if gpu && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-structure-alone.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
}

/// A structure without a type opens on its structure screen ("Strings and points…"):
/// as it is, its items read back unchanged; a point moved goes on the
/// structure on Apply (one command); "Make it a type…" saves it as a
/// structure type of the project and the structure takes it.
#[test]
fn a_structure_without_a_type_is_edited_in_the_pylon_editor() {
    let model = synthetic_project("placed-editor");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().ready().unwrap().selection = Selection::Structure("T3".into());
    harness.run();
    harness
        .get_by_label(crate::properties::STRINGS_AND_POINTS)
        .click();
    harness.run();
    harness.run();
    harness.get_by_label("its own points, wire points and strings, in its axes");
    // The four tables: attachment points, insulators (top, kind,
    // assembly, length, angles…), wire points, set points.
    harness.get_by_label("Strings").click();
    harness.run();
    harness.get_by_label("hangs from");
    harness.get_by_label("angle");
    harness.get_by_label("kind");
    harness.get_by_label("+ New string");
    for sheet in ["Attachment points", "Wire points", "Sets"] {
        // (The sheet's tab: the first of that name.)
        harness.query_all_by_label(sheet).next().unwrap().click();
        harness.run();
    }
    harness.get_by_label("on wire point");
    let near = |a: [f64; 3], b: [f64; 3]| (0..3).all(|k| (a[k] - b[k]).abs() < 1e-3);
    {
        let r = harness.state_mut().ready().unwrap();
        let p = r.placed.as_ref().unwrap();
        assert!(!p.dirty());
        let stored = r.model.stored_line().unwrap().clone();
        let insulators = ohl_structures::InsulatorLibrary::default();
        let back = p.structure(&stored, &insulators).unwrap();
        let s = stored.structure("T3").unwrap();
        assert_eq!(back.points.len(), s.points.len());
        for (a, b) in back.points.iter().zip(&s.points) {
            assert_eq!(a.label, b.label);
            assert!(near(a.position, b.position), "{a:?} {b:?}");
        }
        for (a, b) in back.wires.iter().zip(&s.wires) {
            assert_eq!((&a.label, a.set), (&b.label, b.set));
            assert!(near(a.position, b.position), "{a:?} {b:?}");
        }
        assert_eq!(back.strings.len(), s.strings.len());
        for (a, b) in back.strings.iter().zip(&s.strings) {
            assert_eq!(
                (&a.label, &a.top, &a.wire, a.strain, a.stiffness),
                (&b.label, &b.top, &b.wire, b.strain, b.stiffness)
            );
            assert!((a.weight - b.weight).abs() < 1e-6, "{a:?} {b:?}");
        }
    }
    // The first point 1 m higher.
    let (label, before) = {
        let r = harness.state_mut().ready().unwrap();
        let p = r.placed.as_mut().unwrap();
        let t = p.doc["point"].as_array_of_tables_mut().unwrap();
        let first = t.get_mut(0).unwrap();
        let z: f64 = first["point"]
            .as_array()
            .unwrap()
            .get(2)
            .unwrap()
            .as_str()
            .unwrap()
            .trim_end_matches(" m")
            .parse()
            .unwrap();
        let mut a = first["point"].as_array().unwrap().clone();
        a.replace(2, format!("{:.3} m", z + 1.0));
        first["point"] = toml_edit::value(a);
        let label = first["label"].as_str().unwrap().to_owned();
        let s = r.model.line.structure("T3").unwrap();
        let before = s.points.iter().find(|q| q.label == label).unwrap().position;
        (label, before)
    };
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.line.structure("T3").unwrap();
        let after = s.points.iter().find(|q| q.label == label).unwrap().position;
        assert!(
            near(after, [before[0], before[1], before[2] + 1.0]),
            "{after:?}"
        );
        // Its insulators' wire points came along (their lengths hold).
        for x in s.strings.iter().filter(|x| x.top == label) {
            let w = s.wire(&x.wire).unwrap().position;
            let d = (0..3)
                .map(|k| (w[k] - after[k]).powi(2))
                .sum::<f64>()
                .sqrt();
            assert!((d - x.length).abs() < 1e-6, "{} {d}", x.label);
        }
        assert!(!r.placed.as_ref().unwrap().dirty());
        r.placed.as_mut().unwrap().type_id = Some("my-pylon".into());
    }
    harness.run();
    harness.get_by_label("Save and use").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(
        r.placed.is_none(),
        "{:?} {:?} {}",
        r.status,
        r.placed.as_ref().map(|p| p.message.clone()),
        r.placed
            .as_ref()
            .map_or(String::new(), |p| p.doc.to_string())
    );
    let s = r.model.line.structure("T3").unwrap();
    assert_eq!(s.structure_type.as_deref(), Some("my-pylon"));
    let after = s.points.iter().find(|q| q.label == label).unwrap().position;
    assert!(
        near(after, [before[0], before[1], before[2] + 1.0]),
        "{after:?}"
    );
    assert_eq!(s.strings.len(), 3);
    let stored = r.model.stored_line().unwrap().structure("T3").unwrap();
    assert!(stored.points.is_empty() && stored.strings.is_empty());
}

/// Two sets of a structure swapped in its editor (Set points): on Apply
/// the sections and jumpers strung there follow, in one command.
#[test]
fn swapped_sets_take_their_wires_along() {
    let model = synthetic_project("set-swap");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (before, a, b) = {
        let r = harness.state_mut().ready().unwrap();
        let line = r.model.stored_line().unwrap().clone();
        let s = line.structure("T3").unwrap();
        let sets: Vec<u32> = s.sets().collect();
        let (a, b) = (sets[0], sets[1]);
        let mut p = crate::placed_editor::PlacedEditor::open(&line, "T3").unwrap();
        // Swap the two sets on their wire points.
        let wires = p.doc["wire"].as_array_of_tables_mut().unwrap();
        for w in wires.iter_mut() {
            let set = w.get("set").and_then(toml_edit::Item::as_integer);
            if set == Some(i64::from(a)) {
                w["set"] = toml_edit::value(i64::from(b));
            } else if set == Some(i64::from(b)) {
                w["set"] = toml_edit::value(i64::from(a));
            }
        }
        let insulators = ohl_structures::InsulatorLibrary::default();
        let (what, command) = p.command(&line, &insulators).unwrap();
        assert!(what.contains(&format!("{a}➡{b}")), "{what}");
        assert!(
            r.perform(crate::edit::Action::Command { what, command }),
            "{:?}",
            r.status
        );
        (line, a, b)
    };
    let r = harness.state_mut().ready().unwrap();
    let after = r.model.stored_line().unwrap();
    let swap = |p: &ohl_line::SectionPoint| {
        let mut p = p.clone();
        if p.structure == "T3" {
            p.set = if p.set == a {
                b
            } else if p.set == b {
                a
            } else {
                p.set
            };
        }
        p
    };
    for (x, y) in before.sections.iter().zip(&after.sections) {
        let want: Vec<ohl_line::SectionPoint> = x.points.iter().map(swap).collect();
        assert_eq!(y.points, want, "{}", x.id);
    }
    for (x, y) in before.jumpers.iter().zip(&after.jumpers) {
        assert_eq!(
            (swap(x.from.point().unwrap()), swap(x.to.point().unwrap())),
            (
                y.from.point().unwrap().clone(),
                y.to.point().unwrap().clone()
            )
        );
    }
}

/// A string's own wire point renamed in the string window: its string
/// follows each name typed, so its fields stay; an empty or taken name
/// waits (not applied) and is dropped when the field is left.
#[test]
fn a_wire_point_renamed_keeps_its_string() {
    use egui::Key;
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.editor = crate::editors::StringForm::of_string(&r.model, "T2", "S1")
            .map(crate::editors::Form::String);
    }
    harness.run();
    let form = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        let Some(crate::editors::Form::String(f)) = &r.editor else {
            panic!()
        };
        f.clone()
    };
    let f = form(&mut harness);
    let wire = f.strings[0].wire.clone();
    let other = f
        .points
        .iter()
        .map(|p| p.label.clone())
        .find(|l| *l != wire)
        .unwrap();
    fn field(v: &str) -> impl Fn(&egui_kittest::kittest::AccessKitNode<'_>) -> bool + '_ {
        move |n| n.role() == egui::accesskit::Role::TextInput && n.value().as_deref() == Some(v)
    }
    harness.get_by(field(&wire)).focus();
    harness.run();
    // Every letter away: the field is empty, the string keeps its wire
    // point and its fields.
    for _ in 0..wire.len() + 2 {
        harness.key_press(Key::Backspace);
        harness.run();
    }
    let f = form(&mut harness);
    assert!(
        f.wires.iter().any(|w| w.label == f.strings[0].wire),
        "{f:?}"
    );
    harness.get_by_label("clamp");
    harness.get_by_label("counterweight");
    // A taken name waits.
    // (typed into the field still focused)
    harness.event(egui::Event::Text(other.clone()));
    harness.run();
    let f = form(&mut harness);
    assert_ne!(f.strings[0].wire, other);
    assert!(f.wires.iter().any(|w| w.label == f.strings[0].wire));
    // A free one is applied, the string with it.
    harness.event(egui::Event::Text("-new".into()));
    harness.run();
    let f = form(&mut harness);
    let new = format!("{other}-new");
    assert_eq!(f.strings[0].wire, new);
    assert!(f.wires.iter().any(|w| w.label == new));
    harness.get_by_label("clamp");
}

/// A new project starts with its own empty wire library (not the
/// examples shipped with the program); weather cases are added in the
/// table, Apply keeps them in the project as its first case file.
#[test]
fn a_new_project_starts_empty_and_takes_weather_cases() {
    use crate::tables::TableKind;
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-fresh-libs", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fresh.ohl");
    std::fs::remove_file(&path).ok();
    crate::files::create_project(&path, "Fresh line", "EPSG:2056", "tester").unwrap();
    // Library files lie next to it and are given (as by a previous
    // project): not taken in.
    std::fs::copy(
        root().join("libraries/cables/acsr-examples.ohlw"),
        dir.join("around.ohlw"),
    )
    .unwrap();
    let sources = Sources {
        library: Some(root().join("libraries/cables")),
        weather: Some(root().join("examples/synthetic-line/weather-cases.ohlwc")),
        file: path.clone(),
        point_budget: 1000,
        corridor: 60.0,
        user: Some("tester".into()),
        ..Sources::default()
    };
    let model = model::load(sources.clone(), &|_| {}).unwrap();
    assert!(
        model.library.is_empty(),
        "{:?}",
        model.library.iter().map(|c| &c.id).collect::<Vec<_>>()
    );
    assert_eq!(model.cases.len(), 1);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let mut row = TableKind::WeatherCases.new_row(&r.model, r.case, &[]);
        row[1] = "cold".into();
        r.tables
            .entry(TableKind::WeatherCases)
            .or_default()
            .added
            .push(row);
    }
    apply_table(&mut harness, TableKind::WeatherCases, true).unwrap();
    let names = |m: &model::Model| {
        m.cases
            .iter()
            .map(|c| c.case.name.clone())
            .collect::<Vec<_>>()
    };
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(names(&r.model), ["15 °C", "cold"]);
    drop(harness);
    // Kept in the project: opened again, both cases are there.
    let model = model::load(sources, &|_| {}).unwrap();
    assert_eq!(names(&model), ["15 °C", "cold"]);
    assert!(model.library.is_empty());
}

/// The structure editor opens in a window of its own (it can be docked).
#[test]
fn the_structure_editor_opens_in_a_window() {
    let mut dock = crate::app::default_dock();
    crate::app::open_page(&mut dock, crate::app::Tab::StructureEditor);
    let at = dock.find_tab(&crate::app::Tab::StructureEditor).unwrap();
    assert!(!at.surface.is_main());
}

/// Clearances are in the Checks menu only (not under Sections): ground
/// clearances (the two windows) and wire clearances (a rule file's checks).
#[test]
fn clearances_are_in_the_checks_menu() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    crate::tests::line_menu(&mut harness, "Sections");
    harness.run();
    assert!(harness.query_by_label("Vertical clearances…").is_none());
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    open_clearance_window(&mut harness, "Radial clearances…");
    assert!(harness.state_mut().ready().unwrap().radial_clearances.open);
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label_contains("Wire clearances").hover();
    harness.run();
    harness.get_by_label("Show results");
    harness.get_by_label("also after every edit");
}

/// Tools › Add structure holds the tool; each use opens the window, one
/// Apply adds the structure (the mouse free after it); again for the next
/// one.
#[test]
fn add_structure_from_the_menu_takes_one_click_each() {
    let model = synthetic_project("add-structure-menu");
    let n = model.line.structures.len();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // First-person controls: a press in the 3D view captures the mouse.
    harness.state_mut().ready().unwrap().view3d.controls =
        crate::controls::Controls::by_name("minecraft").unwrap();
    for k in 1..=2 {
        crate::tests::submenu(&mut harness, "Edit", "Tools");
        harness.run();
        harness.get_by_label("Add structure").click();
        harness.run();
        {
            let r = harness.state_mut().ready().unwrap();
            assert_eq!(r.hotbar.tool(), crate::tools::Tool::AddStructure);
            r.use_tool(crate::selection::Use {
                target: Selection::None,
                at: None,
                on: None,
            });
        }
        harness.run();
        assert!(
            harness.state_mut().ready().unwrap().editor.is_some(),
            "window {k} opened"
        );
        harness.get_by_label("Apply").click();
        harness.run();
        let r = harness.state_mut().ready().unwrap();
        assert!(!r.view3d.captured, "the mouse is free after Apply");
        assert!(r.editor.is_none(), "{:?}", r.status);
        assert_eq!(r.model.line.structures.len(), n + k, "{:?}", r.status);
    }
}

/// Opening a menu stops what was going on, as Escape would: the mouse is
/// freed, an alignment being laid out is given up.
#[test]
fn opening_a_menu_escapes_what_was_going_on() {
    let model = synthetic_project("menu-escapes");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.view3d.captured = true;
        r.new_alignment = vec![ohl_line::AlignmentPoint::free("", [0.0, 0.0, 500.0])];
    }
    harness.run();
    crate::tests::line_menu(&mut harness, "Structures");
    let r = harness.state_mut().ready().unwrap();
    assert!(!r.view3d.captured);
    assert!(r.new_alignment.is_empty());
    harness.get_by_label("Spot structures…");
}

/// No tool twice: the Structures menu holds no tool nor the commands that
/// moved to the Tools menu (Add structure…, New pylon from a template…,
/// Place on alignment points…); the Tools menu has every tool and New
/// pylon from a template…, which opens the pylon builder.
#[test]
fn the_structures_menu_holds_no_tools() {
    use crate::tools::Tool;
    let model = synthetic_project("menus-no-duplicates");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let gone = [
        "Add structure…",
        "New structure from a template…",
        "Place on alignment points…",
    ];
    let names: Vec<&str> = Tool::ALL.iter().map(|t| t.name()).chain(gone).collect();
    // How often each name is shown with no menu open ("Edit" is a menu of
    // the bar).
    let count = |h: &Harness<'_, App>| -> Vec<usize> {
        names
            .iter()
            .map(|n| h.query_all_by_label_contains(n).count())
            .collect()
    };
    let closed = count(&harness);
    crate::tests::line_menu(&mut harness, "Structures");
    harness.get_by_label("Spot structures…");
    assert_eq!(count(&harness), closed, "{names:?}");
    harness.key_press(egui::Key::Escape);
    harness.run();
    crate::tests::submenu(&mut harness, "Edit", "Tools");
    harness.run();
    let open = count(&harness);
    for (k, n) in names.iter().enumerate().take(Tool::ALL.len()) {
        assert!(open[k] > closed[k], "{n} in the Tools menu");
    }
    harness
        .get_by_label("New structure from a template…")
        .click();
    harness.run();
    assert!(harness.state_mut().ready().unwrap().pylon_builder.is_some());
}

/// A quick bar saved with a tool and New pylon from a template loads with
/// both (the menu entries removed from Structures were never quick-bar
/// commands; the tool's key is unchanged).
#[test]
fn saved_quick_bars_keep_the_structure_tools() {
    use crate::menu::{Command, QuickBar};
    use crate::tools::Tool;
    let mut storage = Memory::default();
    eframe::Storage::set_string(
        &mut storage,
        "quickbar",
        "undo,tool-add_structure,new-pylon,save".into(),
    );
    let bar = QuickBar::load(&storage).unwrap();
    assert_eq!(
        bar.0,
        [
            Command::Undo,
            Command::Hold(Tool::AddStructure),
            Command::NewPylon,
            Command::Save
        ]
    );
    let mut again = Memory::default();
    bar.save(&mut again);
    assert_eq!(QuickBar::load(&again), Some(bar));
}

/// Add structure on an alignment point with the base not on the ground:
/// it and the other points ticked stand at the points' own heights, the
/// window's type, extension and height adjustment on each (a row's own
/// type: its base); with no type at all, Apply says which point needs one.
#[test]
fn add_structure_on_more_points_keeps_the_window_s_choices() {
    use crate::editors::{Form, StructureForm};
    let model = typed_project_edited(
        "more-points",
        |_| {},
        |l| {
            l.alignments
                .push(ohl_line::Alignment::new("extra", "main", Vec::new()));
            l.set_alignment_points(
                "extra",
                [[0.0, -80.0], [300.0, -80.0], [600.0, -20.0]].map(|p| ([p[0], p[1], 777.0], None)),
            );
        },
    );
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let before = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .structures
        .len();
    // A type with extensions: the window's extension goes with its type.
    let ty = "ext-02".to_owned();
    {
        let r = harness.state_mut().ready().unwrap();
        let m = &r.model;
        let mut f = StructureForm::new_at(m, [300.0, -80.0, 777.0]);
        assert_eq!(f.on_alignment, Some(("extra".into(), 1)), "the point near");
        f.on_ground = false;
        f.position[2] = 777.0;
        f.structure_type = None;
        let mut more = crate::placement::MorePoints::new(m, "extra", 1);
        assert_eq!(
            more.rows.iter().map(|r| r.index).collect::<Vec<_>>(),
            [0, 2]
        );
        for row in &mut more.rows {
            row.on = true;
        }
        f.more = Some(more);
        let e = Form::Structure(f.clone()).command(m, false).unwrap_err();
        assert!(e.contains("point 1"), "{e}");
        f.structure_type = Some(ty.clone());
        f.extension = Some("+3".into());
        f.height_adjust = 0.5;
        f.more.as_mut().unwrap().rows[1].structure_type = Some(ty.clone());
        r.editor = Some(Form::Structure(f));
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.editor.is_none(), "{:?}", r.status);
    let l = &r.model.line;
    assert_eq!(l.structures.len(), before + 3);
    let pts = l.points_of(l.alignment_by_id("extra").unwrap());
    let on: Vec<&ohl_line::Structure> = pts
        .iter()
        .map(|p| l.structure(p.structure.as_deref().expect("tied")).unwrap())
        .collect();
    for s in &on {
        assert!(
            (s.position[2] - 777.0).abs() < 1e-9,
            "{}: {:?}",
            s.id,
            s.position
        );
        assert_eq!(s.structure_type.as_deref(), Some(ty.as_str()));
    }
    // The type above: the window's extension and foundations; a row's own
    // type: its base.
    assert_eq!(
        (
            on[0].height_adjust,
            on[1].height_adjust,
            on[2].height_adjust
        ),
        (0.5, 0.5, 0.0)
    );
    let ext: Vec<Option<&str>> = on.iter().map(|s| s.extension.as_deref()).collect();
    assert_eq!(ext, [Some("+3"), Some("+3"), None]);
}

/// View › Profile aspect ratio: at 1:10 the heights are drawn ten times
/// larger than the stations, and stay so when zooming (the height under
/// the pointer stays under it); fitted, they follow what is shown.
#[test]
fn the_profile_keeps_its_aspect_ratio_when_zooming() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().show_tab(crate::app::Tab::Profile);
    harness.run_steps(3);
    // Chosen in the menu.
    harness.get_by_label("View").click();
    harness.run();
    harness
        .get_by_label_contains("Profile aspect ratio")
        .hover();
    harness.run();
    harness.get_by_label("1:10 (heights ×10)").click();
    harness.run();
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    assert_eq!(
        harness.state_mut().ready().unwrap().model.profile_ratio,
        Some(10.0)
    );
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .model
            .view_prefs()
            .profile_ratio,
        Some(10.0)
    );
    harness.run_steps(2);
    let shown = |h: &mut Harness<'_, App>| h.state_mut().ready().unwrap().profile.shown.unwrap();
    // Heights per point = 10 × stations per point.
    let ratio = |(s, z, size): ([f64; 2], [f64; 2], [f64; 2])| {
        ((s[1] - s[0]) / size[0]) / ((z[1] - z[0]) / size[1])
    };
    let before = shown(&mut harness);
    assert!((ratio(before) - 10.0).abs() < 1e-6, "{before:?}");
    // Zoom in at a point of the profile.
    let rect = harness.state_mut().ready().unwrap().profile.rect.unwrap();
    let pos = rect.center() + egui::vec2(100.0, 40.0);
    harness.hover_at(pos);
    harness.run();
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 200.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    harness.run_steps(10);
    let after = shown(&mut harness);
    assert!(
        after.0[1] - after.0[0] < before.0[1] - before.0[0],
        "zoomed in: {after:?}"
    );
    assert!((ratio(after) - 10.0).abs() < 1e-6, "{after:?}");
    // Fitted again: the heights follow what is shown.
    harness.state_mut().ready().unwrap().model.profile_ratio = None;
    harness.run_steps(2);
    let fitted = shown(&mut harness);
    assert!((ratio(fitted) - 10.0).abs() > 1e-3, "{fitted:?}");
}

/// A connection away from the alignment (80 m beside it between T1 and
/// T2, on the route because it gives it) does not take T1–T2 out of the
/// profile: its spans are drawn; the connection is shown, projected at
/// its station, only within the line width (±40 m by default: not; at
/// ±100 m: yes).
#[test]
fn the_profile_keeps_the_spans_along_the_alignment_past_a_connection() {
    let mut model = synthetic();
    let mut c = model.line.structure("T1").unwrap().clone();
    c.id = "C1".into();
    c.position = [150.0, -80.0, 490.0];
    c.routes = vec!["main".into()];
    model.line.structures.push(c);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().show_tab(crate::app::Tab::Profile);
    harness.run_steps(3);
    {
        let r = harness.state_mut().ready().unwrap();
        let d = &r.profile.drawing.as_ref().unwrap().1.drawing;
        let ids: Vec<&str> = d.structures.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids[..2], ["T1", "T2"], "C1 outside ±40 m");
        r.drawings.set_line_width(100.0);
    }
    harness.run_steps(3);
    let r = harness.state_mut().ready().unwrap();
    let d = &r.profile.drawing.as_ref().unwrap().1.drawing;
    let ids: Vec<&str> = d.structures.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids[..3], ["T1", "C1", "T2"]);
    let m = &d.structures[1];
    assert!((m.station - 150.0).abs() < 1e-6 && (m.offset - 80.0).abs() < 1e-6);
    for section in ["S1", "S2"] {
        let spans: Vec<&str> = d
            .wires
            .iter()
            .filter(|w| w.label.starts_with(&format!("{section} span ")))
            .map(|w| w.label.split_whitespace().nth(2).unwrap())
            .collect();
        assert_eq!(spans, ["0", "1", "2", "3"], "{section}");
    }
}

/// The profile's corridor is the line width of the drawings (one
/// setting, ±40 m by default): the ground is drawn at both edges and the
/// survey points and LiDAR points within it are shown; at ±20 m (typed in
/// the profile) those within 20 m, and the export draws at ±20 m too.
/// "All alignments, in order" draws the first, then the second, each
/// starting where the one before ends.
#[test]
fn the_profile_shows_a_corridor_and_every_alignment_in_order() {
    let model = synthetic();
    let routes = model.line.alignment_views();
    assert!(routes.len() > 1);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().show_tab(crate::app::Tab::Profile);
    harness.run_steps(3);
    let built = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        let b = &r.profile.drawing.as_ref().unwrap().1;
        (
            b.drawing.sides.len(),
            b.survey.len(),
            b.drawing.length,
            b.joints.clone(),
            b.drawing
                .structures
                .iter()
                .map(|m| (m.id.clone(), m.station))
                .collect::<Vec<_>>(),
        )
    };
    let (sides, wide, single, _, _) = built(&mut harness);
    assert_eq!(sides, 2);
    assert!(wide > 10, "{wide}");
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.drawings.line_width(), 40.0);
        let b = &r.profile.drawing.as_ref().unwrap().1;
        assert_eq!(b.drawing.sides[0].0.abs(), 40.0);
        // The LiDAR points loaded within ±40 m, projected (the synthetic
        // model has none: the same count as found by hand).
        let centre = b.drawing.centreline.clone();
        let near = r
            .model
            .points
            .iter()
            .filter(|(q, _)| {
                let p = [0, 1, 2].map(|k| r.model.origin[k] + f64::from(q[k]) / 1000.0);
                let (st, off) = ohl_line::route::project(&centre, p);
                let back = ohl_line::route::point_at(&centre, st, off);
                off.abs() <= 40.0 && (back[0] - p[0]).hypot(back[1] - p[1]) < 0.4
            })
            .count();
        assert_eq!(b.lidar.len(), near);
    }
    // ±20 m, typed in the profile's own field: the drawings' line width.
    harness.get_by_label("line width ±");
    harness
        .state_mut()
        .ready()
        .unwrap()
        .drawings
        .set_line_width(20.0);
    harness.run_steps(3);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.drawings.settings().line_width, 20.0);
        assert_eq!(r.drawings.stored().line_width, 20.0);
        let w = crate::drawings::DrawingsWindow {
            out: Some(std::env::temp_dir()),
            ..r.drawings.settings().clone()
        };
        let input = crate::drawings::Input::new(&r.model, r.case, &w).unwrap();
        assert_eq!(input.options.plan_half_width, 20.0);
    }
    let (sides, survey, _, _, _) = built(&mut harness);
    assert!(survey < wide, "{survey} < {wide}");
    assert_eq!(sides, 2);
    assert!(survey > 10, "{survey}");
    let in_corridor = {
        let r = harness.state_mut().ready().unwrap();
        let b = &r.profile.drawing.as_ref().unwrap().1;
        let centre = b.drawing.centreline.clone();
        let pts = r.model.ground.as_ref().unwrap().points().to_vec();
        pts.iter()
            .filter(|p| {
                let (st, off) = ohl_line::route::project(&centre, **p);
                let back = ohl_line::route::point_at(&centre, st, off);
                off.abs() <= 20.0 && (back[0] - p[0]).hypot(back[1] - p[1]) < 0.2
            })
            .count()
    };
    assert_eq!(survey, in_corridor);
    // A 3D drawing line across the line, 5 m above the ground, comes in;
    // a flat (2D) one does not; hidden, it is not drawn (kept, filtered).
    {
        let r = harness.state_mut().ready().unwrap();
        let b = &r.profile.drawing.as_ref().unwrap().1;
        let c = &b.drawing.centreline;
        let mid = [0.5 * (c[0][0] + c[1][0]), 0.5 * (c[0][1] + c[1][1])];
        let (dx, dy) = (c[1][0] - c[0][0], c[1][1] - c[0][1]);
        let n = dx.hypot(dy);
        let across = [-dy / n, dx / n];
        let line = |z: f64| ohl_background::dxf::Polyline {
            layer: 0,
            color: [255, 0, 255],
            points: (-4..=4)
                .map(|k| {
                    let t = f64::from(k) * 10.0;
                    [mid[0] + across[0] * t, mid[1] + across[1] * t, z]
                })
                .collect(),
            closed: false,
        };
        let drawing = ohl_background::dxf::Drawing {
            layers: Vec::new(),
            lines: vec![line(505.0), line(0.0)],
            points: Vec::new(),
            texts: Vec::new(),
            skipped: Default::default(),
        };
        r.model
            .backgrounds
            .drawings
            .push(crate::backgrounds::LoadedDrawing {
                info: ohl_background::DrawingInfo {
                    name: "invented-3d".into(),
                    source: String::new(),
                    crs: String::new(),
                    items: 2,
                    bounds: None,
                },
                boxes: Vec::new(),
                drawing: std::sync::Arc::new(drawing),
            });
        r.profile.invalidate();
    }
    harness.run_steps(3);
    {
        let r = harness.state_mut().ready().unwrap();
        let b = &r.profile.drawing.as_ref().unwrap().1;
        assert_eq!(b.lines.len(), 1, "the 3D line only");
        // Its part within ±20 m: 5 points (−20 … 20 m).
        assert_eq!(b.lines[0].2.len(), 5);
        assert!(b.lines[0].2.iter().all(|p| p[1] == 505.0));
    }
    // Every alignment, in order.
    {
        let r = harness.state_mut().ready().unwrap();
        r.profile.show = Some(crate::profile::Show::All);
    }
    harness.run_steps(3);
    let (_, _, total, joints, structures) = built(&mut harness);
    assert_eq!(joints.len(), routes.len());
    assert_eq!(joints[0].1, format!("alignment {}", routes[0].id));
    assert_eq!(joints[1].1, format!("alignment {}", routes[1].id));
    assert!(total > single);
    // The second alignment's structures come after the first's.
    let first_end = joints[1].0;
    for id in &routes[1].structures {
        assert!(
            structures
                .iter()
                .any(|(s, at)| s == id && *at >= first_end - 1e-6),
            "{id}: {structures:?}"
        );
    }
}

/// Terrain › Routes: a route added, one renamed (its alignments follow),
/// one deleted (asked: its alignment goes with it). Structures › routes:
/// from the alignments (dimmed), or ticked for one on no alignment.
#[test]
fn routes_are_made_renamed_and_deleted_and_structures_take_them() {
    use crate::tables::TableKind;
    let model = synthetic_project("routes-table");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::Routes.rows(&r.model, 0);
        let main = rows.iter().find(|x| x.key == "main").unwrap();
        assert_eq!(main.cells[2], "main");
        assert_eq!(main.cells[3], "5", "T1–T5");
        let mut row = TableKind::Routes.new_row(&r.model, 0, &[]);
        assert_eq!(row[0], "route 1");
        row[0] = "north".into();
        row[1] = "North route".into();
        r.tables
            .entry(TableKind::Routes)
            .or_default()
            .added
            .push(row);
    }
    set_cell(&mut harness, TableKind::Routes, "branch", "route", "west");
    apply_table(&mut harness, TableKind::Routes, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let line = &r.model.line;
        assert!(
            line.routes
                .iter()
                .any(|x| x.id == "north" && x.name == "North route")
        );
        let branch = line.alignments.iter().find(|a| a.id == "branch").unwrap();
        assert_eq!(branch.route, "west");
        assert_eq!(line.routes_of("T6"), ["west"]);
    }
    // T6 on north too, ticked in the Structures table.
    {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::Structures.rows(&r.model, 0);
        let t6 = rows.iter().find(|x| x.key == "T6").unwrap();
        let col = TableKind::Structures.column("routes");
        assert_eq!(t6.cells[col], "west");
        assert!(t6.defaulted[col], "from the alignment: dimmed");
    }
    set_cell(
        &mut harness,
        TableKind::Structures,
        "T6",
        "routes",
        "west, north",
    );
    apply_table(&mut harness, TableKind::Structures, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(
            r.model.line.structure("T6").unwrap().routes,
            ["west", "north"]
        );
        assert_eq!(r.model.line.routes_of("T6"), ["north", "west"]);
    }
    // Deleting west: asked, its alignment goes; T6 keeps north.
    {
        let r = harness.state_mut().ready().unwrap();
        r.tables
            .entry(TableKind::Routes)
            .or_default()
            .deleted
            .insert("west".into());
    }
    assert_eq!(
        apply_table(&mut harness, TableKind::Routes, false).unwrap_err(),
        "needs a confirmation"
    );
    apply_table(&mut harness, TableKind::Routes, true).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let line = &r.model.line;
    assert!(line.routes.iter().all(|x| x.id != "west"));
    assert!(line.alignments.iter().all(|a| a.id != "branch"));
    assert_eq!(line.structure("T6").unwrap().routes, ["north"]);
}

/// View › Work on route: the branch chosen, its structures, sections and
/// jumpers are what the checks and loads keep to; the profile shows it;
/// remembered with the display choices.
#[test]
fn work_on_one_route() {
    let model = synthetic_project("work-on-route");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label_contains("Work on route").hover();
    harness.run();
    harness.get_by_label("branch").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.work_route.as_deref(), Some("branch"));
    assert_eq!(r.model.view_prefs().work_route.as_deref(), Some("branch"));
    assert_eq!(
        r.profile.show,
        Some(crate::profile::Show::Route("branch".into()))
    );
    let scope = r.model.scope().unwrap();
    assert_eq!(
        scope
            .structures
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["T3", "T6", "T7"]
    );
    // The branch's sections, and the jumper from main into it (at T3).
    let (sections, _) = r.model.line.wire_routes();
    for (id, routes) in sections {
        assert_eq!(
            scope.sections.contains(&id),
            routes.contains(&"branch".to_owned()),
            "{id}"
        );
    }
    assert!(!scope.jumpers.is_empty());
}
