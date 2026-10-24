// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: Escape, the profile, local timings, the string window, circuit
//! colours, the colour tool.

use super::*;

/// In the 3D view in first person: Escape while stringing cancels the
/// wire and keeps the mouse; a second Escape frees it.
#[test]
fn escape_twice_in_first_person() {
    use crate::tools::Tool;
    if ohl_render::headless_device().is_err() {
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        return;
    }
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .wgpu()
        .build_eframe(|cc| {
            App::with_model(synthetic_project("escape-fp"), cc.wgpu_render_state.clone())
        });
    harness.run();
    harness.state_mut().ready().unwrap().view3d.controls = crate::controls::Controls::presets()
        .into_iter()
        .find(|c| c.name == "Minecraft creative")
        .unwrap();
    harness.state_mut().show_tab(crate::app::Tab::View3d);
    harness.run();
    use_tool(&mut harness, Tool::StringWire, string_at("T2", 1), None);
    harness.state_mut().ready().unwrap().view3d.captured = true;
    harness.run();
    assert!(
        harness.state_mut().ready().unwrap().view3d.captured,
        "captured before Escape"
    );
    assert!(
        !harness.state_mut().ready().unwrap().chain.is_empty(),
        "a point picked"
    );
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.chain.is_empty());
        assert_eq!(r.hotbar.tool(), Tool::StringWire);
        assert!(r.view3d.captured, "the first Escape keeps the mouse");
    }
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(
        !harness.state_mut().ready().unwrap().view3d.captured,
        "the second frees it"
    );
}

/// Escape cancels the wire being strung, at any point (points picked with
/// the mouse captured, or the section window with a field being typed in):
/// the points go, String wire stays held and the view keeps the mouse; a
/// second Escape frees it. The same for a new alignment.
#[test]
fn escape_cancels_the_operation_not_the_tool() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project("escape-string-wire");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let escape = |h: &mut Harness<'_, App>| press(h, egui::Key::Escape, egui::Modifiers::NONE);
    let cancelled = |h: &mut Harness<'_, App>, tool: Tool| {
        let r = h.state_mut().ready().unwrap();
        assert!(r.chain.is_empty() && r.new_alignment.is_empty() && r.view3d.draft.is_empty());
        assert!(r.editor.is_none());
        assert_eq!(r.hotbar.tool(), tool);
    };
    // Points picked, the mouse captured: one Escape cancels, the mouse stays.
    use_tool(&mut harness, Tool::StringWire, string_at("T2", 1), None);
    use_tool(&mut harness, Tool::StringWire, string_at("T3", 1), None);
    harness.state_mut().ready().unwrap().view3d.captured = true;
    escape(&mut harness);
    cancelled(&mut harness, Tool::StringWire);
    assert!(harness.state_mut().ready().unwrap().view3d.captured);
    // The section window, a field focused: closed, String wire held.
    use_tool(&mut harness, Tool::StringWire, string_at("T2", 1), None);
    use_tool(&mut harness, Tool::StringWire, string_at("T3", 1), None);
    harness.state_mut().ready().unwrap().view3d.captured = false;
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(matches!(r.editor, Some(Form::Section(_))), "{:?}", r.editor);
    }
    harness
        .ctx
        .memory_mut(|m| m.request_focus(egui::Id::new("escape-test-field")));
    escape(&mut harness);
    cancelled(&mut harness, Tool::StringWire);
    // A new alignment: the same.
    let id = harness.state_mut().ready().unwrap().model.line.structures[0]
        .id
        .clone();
    use_tool(
        &mut harness,
        Tool::NewAlignment,
        Selection::Structure(id),
        None,
    );
    harness.state_mut().ready().unwrap().view3d.captured = true;
    escape(&mut harness);
    cancelled(&mut harness, Tool::NewAlignment);
    assert!(harness.state_mut().ready().unwrap().view3d.captured);
}

/// Several routes, a short spur listed first: the profile opens on the
/// route with most structures; a route chosen in the list stays.
#[test]
fn the_profile_opens_on_the_main_run() {
    // The branch (3 structures) is listed before main (5).
    let mut model = synthetic_project("profile-main-run");
    model.line.routes.reverse();
    model.line.alignments.reverse();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().show_tab(crate::app::Tab::Profile);
    harness.run_steps(2);
    assert_eq!(
        harness.state_mut().ready().unwrap().profile.show,
        Some(crate::profile::Show::Route("main".into()))
    );
}

/// The plan view of a local project, drawn to a file (local checks only:
/// `OHL_PROJECT`, `OHL_SHOTS`).
#[test]
#[ignore = "local data"]
fn local_plan_image() {
    let (Ok(file), Ok(dir)) = (
        ohl_core::env::var("OHL_PROJECT"),
        ohl_core::env::var("OHL_SHOTS"),
    ) else {
        return;
    };
    let model = model::load(
        Sources {
            file: file.into(),
            point_budget: 20_000,
            corridor: 60.0,
            ..Sources::default()
        },
        &|m| eprintln!("{m}"),
    )
    .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    h.state_mut().show_tab(crate::app::Tab::Plan);
    for _ in 0..30 {
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    h.render().unwrap().save(format!("{dir}/plan.png")).unwrap();
}

/// Where an edit's re-solve goes, on a local project (`OHL_PROJECT`):
/// one section re-solved in a solved case, its parts timed, with the
/// bundles as they are and without their spacing.
#[test]
#[ignore = "local data"]
fn local_resolve_timing() {
    let Ok(file) = ohl_core::env::var("OHL_PROJECT") else {
        return;
    };
    let mut model = model::load(
        Sources {
            file: file.into(),
            point_budget: 1000,
            corridor: 60.0,
            ..Sources::default()
        },
        &|m| eprintln!("{m}"),
    )
    .unwrap();
    let k = model.cases.iter().position(|c| c.ready).unwrap_or(0);
    model.ensure_solved(k);
    let library = model.library.clone();
    // A weather case solved whole: the jumpers' cuts made in the first,
    // kept for the others.
    for c in model.cases.iter().take(3) {
        let shapes: Vec<_> = ohl_line::solve::solve_line(&model.line, &library, &c.case)
            .into_iter()
            .filter_map(Result::ok)
            .collect();
        let t = std::time::Instant::now();
        let r = ohl_line::jumper::solve_jumpers(&model.line, &library, &c.case, &shapes);
        println!(
            "case {}: jumpers {:.1?} ({} failed)",
            c.case.name,
            t.elapsed(),
            r.iter().filter(|r| r.is_err()).count()
        );
    }
    for spaced in [true, false] {
        let mut line = model.line.clone();
        if !spaced {
            for s in &mut line.sections {
                s.bundle_spacing = None;
            }
            for j in &mut line.jumpers {
                j.bundle_spacing = None;
            }
        }
        let mut solved = model::Solved::from_live(
            &line,
            model.cases[k].case.clone(),
            ohl_line::incremental::SolvedLine::solve(&line, &library, &model.cases[k].case),
        );
        let dirty = vec![line.sections[10].id.clone()];
        let t = std::time::Instant::now();
        let shapes = solved.live.shapes(&line);
        let t_shapes = t.elapsed();
        let t = std::time::Instant::now();
        let _ = ohl_line::jumper::solve_jumpers(&line, &library, &solved.case, &shapes);
        let t_jumpers = t.elapsed();
        let t = std::time::Instant::now();
        let _ = ohl_line::bundles::BundleLines::new(&line, &solved.shapes, &solved.jumpers);
        let t_bundles = t.elapsed();
        let t = std::time::Instant::now();
        solved.update(&line, &library, &dirty);
        let t_update = t.elapsed();
        println!(
            "spacing {spaced}: update {t_update:.1?} = shapes {t_shapes:.1?}, jumpers {t_jumpers:.1?}, bundles {t_bundles:.1?}, rest; mirror {}",
            solved.mirror.is_some()
        );
    }
}

#[test]
#[ignore = "writes images of the wire library page scrolled right to OHL_SHOTS (a look by eye)"]
fn wire_library_scrolled_right_image() {
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_eframe(|_| App::with_model(synthetic(), None));
    harness.run();
    harness.state_mut().ready().unwrap().open_page = Some((
        crate::app::Tab::Library(crate::library::LibraryKind::Cables),
        None,
    ));
    harness.run();
    harness.run();
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/wires-0.png"))
        .unwrap();
    let at = harness
        .get_by_label("Bimetallic conductor (outer strands and a core)")
        .rect()
        .center();
    for _ in 0..6 {
        harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(at));
        harness.input_mut().events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(-300.0, 0.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        harness.run_steps(3);
    }
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/wires-1.png"))
        .unwrap();
}

/// The jumpers failing in a local project's first cases (`OHL_PROJECT`):
/// how each is defined, its bundle, its clamps' distance in each case.
#[test]
#[ignore = "local data"]
fn local_jumper_failures() {
    let Ok(file) = ohl_core::env::var("OHL_PROJECT") else {
        return;
    };
    let model = model::load(
        Sources {
            file: file.into(),
            point_budget: 1000,
            corridor: 60.0,
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    let line = &model.line;
    let lib = &model.library;
    let mut failed = std::collections::BTreeSet::new();
    let mut chords: std::collections::BTreeMap<String, Vec<(String, f64, String)>> =
        Default::default();
    for c in model.cases.iter().take(3) {
        let shapes: Vec<_> = ohl_line::solve::solve_line(line, lib, &c.case)
            .into_iter()
            .filter_map(Result::ok)
            .collect();
        for (j, r) in line
            .jumpers
            .iter()
            .zip(ohl_line::jumper::solve_jumpers(line, lib, &c.case, &shapes))
        {
            let msg = match &r {
                Ok(s) => format!("ok, sag {:.3} m, L0 {:.3}", s.sag, s.unstressed_length),
                Err(e) => {
                    failed.insert(j.id.clone());
                    e.to_string()
                }
            };
            chords
                .entry(j.id.clone())
                .or_default()
                .push((c.case.name.clone(), 0.0, msg));
        }
    }
    println!("{} jumpers fail in some of the first 3 cases", failed.len());
    for id in &failed {
        let j = line.jumpers.iter().find(|j| &j.id == id).unwrap();
        let b = line.jumper_bundle(j).count;
        let ends = |p: &ohl_line::SectionPoint| {
            line.sections
                .iter()
                .filter(|s| s.points.contains(p))
                .map(|s| {
                    let k = s.points.iter().position(|q| q == p).unwrap();
                    let end = k == 0 || k + 1 == s.points.len();
                    format!("{}{}", s.id, if end { "(end)" } else { "(through)" })
                })
                .collect::<Vec<_>>()
                .join("+")
        };
        println!(
            "{id} at {}: length {:?} sag {:?} t {} bundle {b}; from {} to {}",
            j.from,
            j.length,
            j.sag,
            j.temperature,
            ends(j.from.point().unwrap()),
            ends(j.to.point().unwrap())
        );
        for (case, _, m) in &chords[id] {
            println!("    {case}: {m}");
        }
    }
}

/// The string editor's presets put on a structure give a valid line: an
/// I-string, a V, a V with a string below, a TAK (two sets); opened again
/// at their set, the same strings come back; a V with a string below
/// strung into a section solves.
#[test]
fn string_editor_presets_make_valid_assemblies() {
    use crate::editors::{Preset, StringForm};
    let mut model = synthetic();
    for preset in Preset::ALL {
        let mut form = StringForm::new_on(&model, "T2").unwrap();
        let set = form.set;
        form.preset(preset, [0.0, 0.0, 40.0], &model);
        let mut line = model.line.clone();
        let s = line.structures.iter_mut().find(|s| s.id == "T2").unwrap();
        let resolved = model.line.structure("T2").cloned();
        form.apply_to(s, resolved.as_ref());
        line.validate()
            .unwrap_or_else(|e| panic!("{preset:?}: {e}"));
        let t2 = line.structure("T2").unwrap();
        let w = t2.wire_at_set(set).unwrap();
        let c = t2.component(&w.label);
        let strings = match preset {
            Preset::I => 1,
            Preset::V => 2,
            Preset::VAndString | Preset::Tak => 3,
        };
        assert_eq!(c.strings.len(), strings, "{preset:?}");
        if preset == Preset::Tak {
            assert!(t2.wire_at_set(set + 1).is_some());
            assert_eq!(
                c.wires
                    .iter()
                    .filter(|&&i| t2.wires[i].set.is_some())
                    .count(),
                2
            );
        }
    }
    // A V with a string below on a new structure, strung from T5's set 1.
    let mut line = model.line.clone();
    let t5 = line.structure("T5").unwrap().clone();
    let mut t9 = ohl_line::Structure::new(
        "T9",
        [t5.position[0] + 250.0, t5.position[1], t5.position[2]],
    );
    t9.orientation_deg = t5.orientation_deg;
    line.structures.push(t9);
    let t10 = ohl_line::Structure {
        wires: vec![ohl_line::WirePoint {
            description: Default::default(),
            label: "W1".into(),
            position: [
                t5.position[0] + 500.0,
                t5.position[1],
                t5.position[2] + 25.0,
            ],
            set: Some(1),
            weight: 0.0,
        }],
        ..ohl_line::Structure::new(
            "T10",
            [t5.position[0] + 500.0, t5.position[1], t5.position[2]],
        )
    };
    line.structures.push(t10);
    model.line = line.clone();
    let mut form = StringForm::new_on(&model, "T9").unwrap();
    form.preset(Preset::VAndString, [0.0, 0.0, 30.0], &model);
    let s = line.structures.iter_mut().find(|s| s.id == "T9").unwrap();
    form.apply_to(s, None);
    let mut section = line.sections[0].clone();
    section.id = "S9".into();
    section.points = vec![
        ohl_line::SectionPoint::new("T5", 1),
        ohl_line::SectionPoint::new("T9", 1),
        ohl_line::SectionPoint::new("T10", 1),
    ];
    section.clear_point_loads();
    section.stringing = ohl_line::Stringing::Tension {
        horizontal_tension: 20_000.0,
        temperature: 15.0,
    };
    line.sections.push(section);
    line.validate().unwrap();
    let case = model.cases[0].case.clone();
    let shape = ohl_line::solve::solve_line(&line, &model.library, &case)
        .pop()
        .unwrap()
        .unwrap();
    assert_eq!(shape.strings[1].len(), 3);
    assert!(shape.strings[1].iter().all(|x| x.force > 0.0));
}

/// The start screen (no project open) opens the plugin manager: a project
/// plugin can be installed or updated before the project it creates.
#[test]
fn the_start_screen_opens_the_plugin_manager() {
    let mut app = App::empty(Sources::default(), None);
    app.plugins = crate::plugins::Plugins::load(None, None);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .build_eframe(|_| app);
    h.run();
    h.get_by_label("Plugins…").click();
    // The manager repaints while it reads the plugins' formats in the
    // background (long on a busy machine): a few frames, not until idle.
    h.run_steps(4);
    assert!(h.state().plugins.open);
    h.get_by_label("Plugin manager");
}

/// A structure's "Strings and points…" (as the Edit tool) opens its type
/// on the structure screen: the tables on the left of the pylon's 3D
/// view (about 45/55), only the basics — attachment points, insulators,
/// wire points, set points; no joints or members — the level and the
/// structures using it. An attachment point added goes into the file as
/// edited (not saved); then "Edit in the structure application…" asks to
/// save first, and once the file is as saved it starts the application on
/// the type's file.
#[test]
fn the_structure_screen_shows_the_basics_left_of_the_pylon() {
    use crate::library::LibraryKind;
    use crate::type_workspace::{EDIT_IN_APP, VIEW_LABEL};
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    let ex = root().join("examples/synthetic-line");
    let model = model::load(
        Sources {
            file: ex.join("synthetic-line-typed.ohl.json"),
            library: Some(root().join("libraries/cables")),
            structures: Some(ex.join("Structures")),
            point_budget: 1000,
            corridor: 60.0,
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    let mut builder = Harness::builder().with_size(egui::vec2(1500.0, 950.0));
    if gpu {
        builder = builder.wgpu();
    }
    let mut harness =
        builder.build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    harness.state_mut().ready().unwrap().selection = Selection::Structure("T2".into());
    harness.run();
    harness
        .get_by_label(crate::properties::STRINGS_AND_POINTS)
        .click();
    harness.run();
    harness.run();
    harness.get_by_label("Level 3 — full design");
    harness.get_by_label("used by 2 structures of the line (they follow when saved)");
    // The basics, and nothing of the body but its drawing.
    for sheet in ["Attachment points", "Strings", "Wire points", "Sets"] {
        assert!(
            harness.query_all_by_label(sheet).next().is_some(),
            "{sheet}"
        );
    }
    for gone in [
        "Joints",
        "Members",
        "Add joint",
        "Member",
        "Point here",
        "support",
    ] {
        assert!(harness.query_by_label(gone).is_none(), "{gone} is shown");
    }
    harness.get_by_label_contains("Body: 46 joint(s), 178 member(s)");
    // Tables left, the view right.
    let tab = harness.get_by_label("Attachment points").rect();
    let header = harness.get_by_label("attachment point").rect();
    let view = harness.get_by_label(VIEW_LABEL).rect();
    assert!(tab.right() < view.left() && header.right() < view.left());
    let tables = view.left() - tab.left();
    let share = tables / (view.right() - tab.left());
    assert!((0.35..0.55).contains(&share), "tables take {share}");
    assert!(view.width() > 400.0 && view.height() > 300.0, "{view:?}");
    if gpu && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-structure-screen.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    // An attachment point beside A1.
    {
        let r = harness.state_mut().ready().unwrap();
        r.structure_apps.dry_run = Some(Vec::new());
        let page = r.libraries.get_mut(&LibraryKind::StructureTypes).unwrap();
        page.workspace.selected = vec![crate::type_workspace::Handle::Point("A1".into())];
    }
    harness.get_by_label("+ Attachment point").click();
    harness.run();
    let path = {
        let r = harness.state_mut().ready().unwrap();
        let page = &r.libraries[&LibraryKind::StructureTypes];
        assert!(page.dirty());
        let text = page.text().unwrap();
        assert_eq!(text.matches("[[point]]").count(), 3, "{text}");
        assert!(page.error.is_none(), "{:?}", page.error);
        page.file.clone().unwrap()
    };
    // Not with changes: the application edits the file as saved.
    harness.get_by_label(EDIT_IN_APP).click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.structure_apps.dry_run.as_deref(), Some(&[][..]));
        let page = r.libraries.get_mut(&LibraryKind::StructureTypes).unwrap();
        let (m, error) = page.message.clone().unwrap();
        assert!(error && m.starts_with("Save or discard"), "{m}");
        // Discarded.
        page.open(&path);
    }
    harness.run();
    harness.get_by_label(EDIT_IN_APP).click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let launched = r.structure_apps.dry_run.clone().unwrap();
    assert_eq!(launched.len(), 1, "{:?}", r.status);
    assert_eq!(launched[0][0], path.as_os_str());
    assert!(launched[0].iter().any(|a| a == "--insulators"));
    assert!(!launched[0].iter().any(|a| a == "--structure"));
}

/// Edit… on a structure without a type: it has no file for the
/// structure application, so its structure screen asks for a type id
/// (Make it a structure file and edit it); Save, use and edit makes it a
/// structure type of the project, the structure takes it, and the
/// structure application is started on that file with the loads of this
/// place.
#[test]
fn edit_on_a_structure_without_a_file_makes_one_first() {
    use crate::placed_editor::MAKE_FILE_AND_EDIT;
    let model = synthetic_project("make-file-and-edit");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.selection = Selection::Structure("T3".into());
        r.structure_apps.dry_run = Some(Vec::new());
    }
    harness.run();
    harness
        .get_by_label(crate::properties::OPEN_IN_STRUCTURE_APP)
        .click();
    harness.run();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let p = r.placed.as_ref().expect("its structure screen");
        assert_eq!((p.id.as_str(), p.then_edit), ("T3", true));
        assert_eq!(p.type_id.as_deref(), Some("T3"));
        assert_eq!(r.structure_apps.dry_run.as_deref(), Some(&[][..]));
        r.placed.as_mut().unwrap().type_id = Some("t3-pylon".into());
    }
    // The screen's own button (no plain Edit in the structure application
    // there: there is no file yet).
    harness.get_by_label(MAKE_FILE_AND_EDIT);
    assert!(
        harness
            .query_by_label(crate::type_workspace::EDIT_IN_APP)
            .is_none()
    );
    harness.get_by_label("Save, use and edit").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.placed.is_none(), "{:?}", r.status);
    let s = r.model.line.structure("T3").unwrap();
    assert_eq!(s.structure_type.as_deref(), Some("t3-pylon"));
    let launched = r.structure_apps.dry_run.clone().unwrap();
    assert_eq!(launched.len(), 1, "{:?}", r.status);
    let args: Vec<String> = launched[0]
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(args[0].ends_with("t3-pylon.ohls"), "{args:?}");
    let k = args.iter().position(|a| a == "--structure").unwrap();
    assert_eq!(args[k + 1], "T3");
}

/// A colour typed in the Sections table goes on every section of its
/// circuit (by name), listed first to confirm; a bad colour is refused.
#[test]
fn a_circuit_takes_one_colour() {
    use crate::tables::TableKind;
    let model = synthetic_project("circuit-colour");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let colours = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.model
            .line
            .sections
            .iter()
            .map(|s| (s.id.clone(), s.circuit_name.clone(), s.color.clone()))
            .collect::<Vec<_>>()
    };
    set_cell(&mut harness, TableKind::Sections, "S1", "colour", "#12AB34");
    assert_eq!(
        apply_table(&mut harness, TableKind::Sections, false).unwrap_err(),
        "needs a confirmation"
    );
    apply_table(&mut harness, TableKind::Sections, true).unwrap();
    for (id, circuit, color) in colours(&mut harness) {
        let want = (circuit == "Example 1").then(|| "#12ab34".to_owned());
        assert_eq!(color, want, "{id} ({circuit})");
    }
    set_cell(&mut harness, TableKind::Sections, "S1", "colour", "red");
    assert!(
        apply_table(&mut harness, TableKind::Sections, true)
            .unwrap_err()
            .contains("#rrggbb")
    );
}

/// Wires coloured by system: the system's colour, its phases in line
/// codes (S1 L1 solid, S2 L2 ·–·–), the branch (joined to S1) like S1;
/// the 3D view and plan draw the codes and arrows (OHL_RENDER_PNG: an
/// image).
#[test]
fn wires_by_system_with_line_codes_and_arrows() {
    use crate::annotations::View;
    use crate::wire_style::{Coloring, Styles};
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    let mut model = synthetic();
    let styles = Styles::new(&model.line, Coloring::Systems);
    let colour = crate::scene::rgb(&model.line.system_color(0));
    let solid: Vec<f64> = Vec::new();
    assert_eq!(
        styles.section("S1"),
        crate::wire_style::Style {
            color: colour,
            pattern: solid.clone()
        }
    );
    assert_eq!(
        styles.section("S2").pattern,
        crate::wire_style::pattern(".-")
    );
    assert_eq!(styles.section("S2").color, colour);
    assert_eq!(styles.section("S3").pattern, solid);
    assert_eq!(styles.jumper("J-T3-branch").color, colour);
    // By circuit: each section's own (phase) colours.
    let by_circuit = Styles::new(&model.line, Coloring::Circuits);
    assert_ne!(
        by_circuit.section("S1").color,
        by_circuit.section("S2").color
    );
    model.section_display.coloring = Coloring::Systems;
    model.section_display.direction = [true; 3];
    let lines = |m: &model::Model| {
        let mut out = Vec::new();
        crate::scene::wires(&mut out, m, &m.line, &m.cases, 0, true, None);
        out.len()
    };
    let coded = lines(&model);
    let display = model.section_display.clone();
    model.section_display.coloring = Coloring::Circuits;
    model.section_display.direction[View::ThreeD as usize] = false;
    let plain = lines(&model);
    assert!(coded > plain + 20, "dashes and arrows: {coded} vs {plain}");
    model.section_display = display;
    let mut builder = Harness::builder().with_size(egui::vec2(1400.0, 900.0));
    if gpu {
        builder = builder.wgpu();
    }
    let mut harness =
        builder.build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    if gpu && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-system-codes.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
}

/// The circuits table colours a whole circuit at once; the systems table
/// gives each phase its line code ("- --." for L1 and L2).
#[test]
fn circuit_colours_and_system_line_codes_in_the_tables() {
    use crate::tables::TableKind;
    use crate::wire_style::{Coloring, Styles, pattern};
    let model = synthetic_project("circuit-codes");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // The circuit of S1 (with the branch S3 by its jumper).
    let key = {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::ConductorPaths.rows(&r.model, 0);
        rows.iter()
            .find(|x| x.cells[6].contains("S3"))
            .unwrap()
            .key
            .clone()
    };
    set_cell(
        &mut harness,
        TableKind::ConductorPaths,
        &key,
        "colour",
        "#AA5500",
    );
    apply_table(&mut harness, TableKind::ConductorPaths, true).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let colour = |id: &str| {
            r.model
                .line
                .sections
                .iter()
                .find(|s| s.id == id)
                .unwrap()
                .color
                .clone()
        };
        assert_eq!(colour("S1").as_deref(), Some("#aa5500"));
        assert_eq!(colour("S3").as_deref(), Some("#aa5500"));
        assert_eq!(colour("S2"), None);
    }
    set_cell(
        &mut harness,
        TableKind::Systems,
        "ex220",
        "line codes",
        "- --.",
    );
    apply_table(&mut harness, TableKind::Systems, true).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let styles = Styles::new(&r.model.line, Coloring::Systems);
    assert_eq!(styles.section("S2").pattern, pattern("--."));
    assert!(styles.section("S1").pattern.is_empty());
    // Not dots and dashes: refused.
    set_cell(
        &mut harness,
        TableKind::Systems,
        "ex220",
        "line codes",
        "- x",
    );
    assert!(
        apply_table(&mut harness, TableKind::Systems, true)
            .unwrap_err()
            .contains("dots and dashes")
    );
}

/// A string is picked all along its body, where it is drawn; the string
/// window places a wire point by angles and a length from the string's
/// top, and its set's dead end (yes/no) makes its strings tension strings
/// or suspension ones; a string whose wire point has no set (a jumper's
/// helping string) opens too.
#[test]
fn strings_are_picked_along_their_body_and_edited_by_angles() {
    use crate::editors::{StringForm, from_polar, polar};
    use crate::selection::{Snaps, candidates, pick};
    // Angles: 0° down, 90° level; direction 0° ahead, 90° to the right.
    let top = [1.0, 2.0, 30.0];
    for (down, dir, len) in [(0.0, 0.0, 4.0), (90.0, 90.0, 2.0), (30.0, 225.0, 3.5)] {
        let at = from_polar(top, down, dir, len);
        let (d, r, l) = polar(top, at);
        assert!((d - down).abs() < 1e-9 && (l - len).abs() < 1e-9, "{d} {l}");
        if down > 0.0 {
            assert!((r - dir).abs() < 1e-9, "{r}");
        }
    }
    let p = from_polar([0.0; 3], 90.0, 90.0, 2.0);
    assert!((p[0] - 2.0).abs() < 1e-12 && p[1].abs() < 1e-12 && p[2].abs() < 1e-12);

    let mut model = synthetic();
    // T2's first string, at its middle as drawn: the string.
    let pieces = crate::selection::string_pieces(&model, 0);
    let (a, b) = pieces[&("T2".to_owned(), "S1".to_owned())][0];
    let mid = [0, 1, 2].map(|k| 0.5 * (a[k] + b[k]));
    // Seen along the line (it runs east: y, z on screen), 1 px per cm.
    let project = |p: [f64; 3]| Some([p[1] * 100.0, -p[2] * 100.0]);
    let at = project(mid).unwrap();
    assert_eq!(
        pick(&candidates(&model, 0, &Snaps::default()), at, 5.0, project),
        Selection::String {
            structure: "T2".into(),
            label: "S1".into()
        }
    );
    // Its window: the wire point from its top; the set a suspension point.
    let f = StringForm::of_string(&model, "T2", "S1").unwrap();
    let w = f.wires.iter().find(|w| w.set.is_some()).unwrap();
    assert!(!w.dead_end);
    // Made a dead end: its strings become tension strings.
    let mut f2 = f.clone();
    f2.wires
        .iter_mut()
        .for_each(|w| w.dead_end = w.set.is_some());
    let mut s = model.line.structure("T2").unwrap().clone();
    f2.apply_to(&mut s, None);
    assert!(s.strings_to(&f2.wires[0].label).all(|x| x.strain));
    // A wire point without a set (a helping string): it opens, set 0.
    let st = model
        .line
        .structures
        .iter_mut()
        .find(|s| s.id == "T3")
        .unwrap();
    let w0 = st.wires[0].position;
    st.wires.push(ohl_line::WirePoint {
        description: Default::default(),
        label: "JH".into(),
        position: [w0[0], w0[1], w0[2] - 2.0],
        set: None,
        weight: 0.0,
    });
    st.strings.push(ohl_line::InsulatorString {
        label: "H1".into(),
        top: st.points[0].label.clone(),
        wire: "JH".into(),
        ..Default::default()
    });
    let h = StringForm::of_string(&model, "T3", "H1").unwrap();
    assert_eq!(h.set, 0);
    assert_eq!(h.strings.len(), 1);
}

/// A string's limits (kept, not checked) in the Strings table: typed in,
/// applied to the stored line, cleared again; a bad one refused; no other
/// string given any; nothing flagged by them.
#[test]
fn string_limits_are_edited_in_the_strings_table() {
    use crate::tables::TableKind;
    let k = TableKind::Strings;
    let model = synthetic_project("string-limits-table");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    for col in ["max load N", "max swing °", "max across °", "max along °"] {
        let r = harness.state_mut().ready().unwrap();
        let rows = k.rows(&r.model, r.case);
        assert!(
            rows.iter().all(|x| x.cells[k.column(col)].is_empty()),
            "{col}"
        );
    }
    set_cell(&mut harness, k, "T2\tS1", "max load N", "120000");
    set_cell(&mut harness, k, "T2\tS1", "max across °", "40.5");
    apply_table(&mut harness, k, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let stored = r.model.stored_line().unwrap();
        let l = stored.string_limits("T2", "S1").unwrap();
        assert_eq!(l.max_load, Some(120_000.0));
        assert_eq!(l.max_swing_across_deg, Some(40.5));
        assert_eq!((l.max_swing_deg, l.max_swing_along_deg), (None, None));
        let given: usize = stored
            .structures
            .iter()
            .flat_map(|s| &s.strings)
            .filter(|x| !x.limits().is_empty())
            .count();
        assert_eq!(given, 1);
        assert_eq!(r.model.line.string_limits("T2", "S1"), Some(l));
        let rows = k.rows(&r.model, r.case);
        let row = rows.iter().find(|x| x.key == "T2\tS1").unwrap();
        assert_eq!(row.cells[k.column("max load N")], "120000");
        assert!(!row.defaulted[k.column("max load N")]);
    }
    // Out of range: refused, nothing changed.
    set_cell(&mut harness, k, "T2\tS1", "max swing °", "200");
    assert!(apply_table(&mut harness, k, false).is_err());
    harness.state_mut().ready().unwrap().tables.remove(&k);
    set_cell(&mut harness, k, "T2\tS1", "max load N", "");
    apply_table(&mut harness, k, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let l = r
        .model
        .stored_line()
        .unwrap()
        .string_limits("T2", "S1")
        .unwrap();
    assert_eq!((l.max_load, l.max_swing_across_deg), (None, Some(40.5)));
}

/// The string window shows a string's limits (kept, not checked), each
/// optional; given in the form, applied to the structure's string.
#[test]
fn the_string_window_edits_limits() {
    let model = synthetic();
    let mut f = crate::editors::StringForm::of_string(&model, "T2", "S1").unwrap();
    let k = f.strings.iter().position(|x| x.label == "S1").unwrap();
    assert!(f.strings[k].limits.is_empty());
    f.strings[k].limits.max_load = Some(90_000.0);
    f.strings[k].limits.max_swing_deg = Some(50.0);
    let mut s = model.line.structure("T2").unwrap().clone();
    f.apply_to(&mut s, None);
    let l = s.string("S1").unwrap().limits();
    assert_eq!((l.max_load, l.max_swing_deg), (Some(90_000.0), Some(50.0)));
    assert_eq!(l.max_swing_across_deg, None);
    // The other strings: none given.
    assert!(
        s.strings
            .iter()
            .filter(|x| x.label != "S1")
            .all(|x| x.limits().is_empty())
    );
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.editor = Some(crate::editors::Form::String(f));
    }
    harness.run();
    harness.get_by_label("max load");
    harness.get_by_label("max swing");
}

/// The string window: where the string hangs (a list of the structure's
/// points), its wire point by angles and length, the set's dead end, with
/// short explanations (OHL_RENDER_PNG: an image).
#[test]
fn the_string_window_explains_itself() {
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
    harness.get_by_label("hangs from");
    harness.get_by_label("clamp");
    harness.get_by_label("down/up");
    // A new string makes its own wire point, 3 m straight down from its
    // top; taken away, its wire point goes with it.
    let before = {
        let r = harness.state_mut().ready().unwrap();
        let Some(crate::editors::Form::String(f)) = &r.editor else {
            panic!()
        };
        (f.strings.len(), f.wires.len())
    };
    harness.get_by_label("+ string").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(crate::editors::Form::String(f)) = &mut r.editor else {
            panic!()
        };
        assert_eq!(
            (f.strings.len(), f.wires.len()),
            (before.0 + 1, before.1 + 1)
        );
        let x = f.strings.last().unwrap().clone();
        let top = f.points.iter().find(|p| p.label == x.top).unwrap().at;
        let w = f.wires.iter().find(|w| w.label == x.wire).unwrap();
        assert_eq!(crate::editors::polar(top, w.at), (0.0, 0.0, 3.0));
        // From the library: a 4.2 m tension assembly, the wire point at it.
        let k = f.strings.len() - 1;
        f.wires.iter_mut().find(|w| w.label == x.wire).unwrap().set = Some(7);
        f.take_from_library(
            k,
            &crate::editors::LibraryString {
                id: "S-420".into(),
                length: 4.2,
                weight: 800.0,
                wind_area: 0.3,
                stiffness: 4e7,
                strain: true,
            },
        );
        let w = f.wires.iter().find(|w| w.label == x.wire).unwrap();
        assert!((crate::editors::polar(top, w.at).2 - 4.2).abs() < 1e-9 && w.dead_end);
        assert_eq!(f.strings[k].weight, 800.0);
        f.strings.pop();
        f.drop_unused_wires();
        assert_eq!((f.strings.len(), f.wires.len()), before);
    }
    harness.run();
    assert_eq!(harness.query_all_by_label("direction").count(), 1);
    if let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-string-window.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
}

/// The colour cells of the tables open the colour tool; a palette colour
/// picked and OK write it in the cell (pending until Apply), which Apply
/// sets (OHL_RENDER_PNG: an image of the tool).
#[test]
fn the_colour_tool_writes_a_palette_colour_in_the_cell() {
    use crate::tables::{ColorCell, TableKind};
    let model = synthetic_project("colour-tool");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().ready().unwrap().open_page =
        Some((crate::app::Tab::Table(TableKind::Systems), None));
    harness.run();
    harness.run();
    // The systems' colours are defaults shown (dimmed): picked, set.
    harness
        .query_all_by_label("colour tool")
        .next()
        .expect("a colour cell")
        .click();
    harness.run();
    let key = {
        let r = harness.state_mut().ready().unwrap();
        match &r.tables[&TableKind::Systems].color {
            Some((ColorCell::Row { key, .. }, _)) => key.clone(),
            other => panic!("{:?}", other.as_ref().map(|c| &c.0)),
        }
    };
    harness.get_by_label("Wheel");
    harness.get_by_label("Sliders");
    // Opening it changes nothing.
    let unchanged = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        let t = &r.tables[&TableKind::Systems].color.as_ref().unwrap().1;
        t.old == Some(t.rgb)
    };
    assert!(unchanged(&mut harness));
    harness.get_by_label("Sliders").click();
    harness.run();
    assert!(unchanged(&mut harness));
    harness.get_by_label("Wheel").click();
    harness.run();
    harness.get_by_label("Neutral").click();
    harness.run();
    if let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-colour-tool.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    harness.get_by_label("OK").click();
    harness.run();
    let c = TableKind::Systems.column("colour");
    {
        let r = harness.state_mut().ready().unwrap();
        let page = &r.tables[&TableKind::Systems];
        assert!(page.color.is_none(), "OK closes it");
        assert_eq!(page.edits[&key][&c], "#b0b8c0");
    }
    apply_table(&mut harness, TableKind::Systems, true).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let s = r.model.line.systems.iter().find(|s| s.id == key).unwrap();
    assert_eq!(s.color.as_deref(), Some("#b0b8c0"));
    // Cancel leaves the cell.
    let page = r.tables.get_mut(&TableKind::Systems).unwrap();
    page.open_color_tool(
        TableKind::Systems,
        ColorCell::Row {
            key: key.clone(),
            column: c,
            original: "#b0b8c0".into(),
        },
        "#b0b8c0",
    );
    harness.run();
    harness.get_by_label("Cancel").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let page = &r.tables[&TableKind::Systems];
    assert!(page.color.is_none() && page.edits.is_empty());
}

/// Every colour reads back the same from hue, saturation and value; the
/// wheel has red to the right, green up-left, saturation outwards.
#[test]
fn colours_go_round_hsv_and_the_wheel() {
    use crate::color_tool::{hsv_to_rgb, rgb_to_hsv, wheel_hs, wheel_point};
    for r in (0..=255u8).step_by(5) {
        for g in (0..=255u8).step_by(15) {
            for b in (0..=255u8).step_by(17) {
                let c = [r, g, b];
                assert_eq!(hsv_to_rgb(rgb_to_hsv(c)), c);
            }
        }
    }
    let hex = "#4c9be8";
    let c = ohl_core::text::parse_rgb(hex).unwrap();
    assert_eq!(ohl_core::text::format_rgb(hsv_to_rgb(rgb_to_hsv(c))), hex);
    assert_eq!(hsv_to_rgb([120.0, 1.0, 1.0]), [0, 255, 0]);
    let [h, s, v] = rgb_to_hsv([0, 0, 255]);
    assert!((h - 240.0).abs() < 1e-9 && s == 1.0 && v == 1.0);
    let o = egui::pos2(100.0, 100.0);
    let near = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3;
    assert!(near(
        wheel_hs(o, 50.0, egui::pos2(125.0, 100.0)),
        (0.0, 0.5)
    ));
    assert!(near(
        wheel_hs(o, 50.0, egui::pos2(100.0, 50.0)),
        (90.0, 1.0)
    ));
    assert!(near(
        wheel_hs(o, 50.0, egui::pos2(60.0, 100.0)),
        (180.0, 0.8)
    ));
    assert!(near(
        wheel_hs(o, 50.0, egui::pos2(100.0, 200.0)),
        (270.0, 1.0)
    ));
    assert!(near(wheel_hs(o, 50.0, o), (0.0, 0.0)));
    let p = wheel_point(o, 50.0, 120.0, 0.6);
    assert!(near(wheel_hs(o, 50.0, p), (120.0, 0.6)));
    // Green (120°) is up and to the left.
    assert!(p.x < o.x && p.y < o.y);
}

/// The palette is written and read back as a file; a file that is not a
/// palette, or has bad colours, is refused with what is wrong; an import
/// replaces the palette or adds what is missing.
#[test]
fn palettes_export_and_import() {
    use crate::color_tool::{ColorTool, Palette, PaletteEntry};
    let mut p = Palette::default();
    assert_eq!(p.entries.len(), 3 + 1 + 8);
    p.entries.push(PaletteEntry {
        name: "Earth \"OPGW\"".into(),
        color: [1, 2, 3],
        description: "earth wires, 380 kV".into(),
    });
    let text = p.to_toml();
    assert!(text.contains("format = \"ohl-colors\""), "{text}");
    assert_eq!(Palette::parse(&text).unwrap(), p.entries);
    // Remembered.
    let mut storage = Memory::default();
    p.mode = crate::color_tool::Mode::Sliders;
    p.save(&mut storage);
    assert_eq!(Palette::load(&storage), p);
    assert_eq!(Palette::load(&Memory::default()), Palette::default());
    // Refused.
    let e = Palette::parse("format = \"ohl-colours\"\nversion = 1\n").unwrap_err();
    assert!(e.contains("is not \"ohl-colors\""), "{e}");
    // The header optional, as every format's.
    assert_eq!(Palette::parse("version = 1\n").unwrap(), Vec::new());
    assert!(
        Palette::parse("format = \"ohl-colors\"\nversion = 9\n")
            .unwrap_err()
            .contains("newer")
    );
    let e = Palette::parse(
        "format = \"ohl-colors\"\nversion = 1\n[[color]]\nname = \"a\"\ncolor = \"red\"\n\
         [[color]]\ncolor = \"#123456\"\n",
    )
    .unwrap_err();
    assert!(
        e.contains("colour 1 (a): color \"red\"") && e.contains("colour 2: name"),
        "{e}"
    );
    assert!(Palette::parse("[[[").unwrap_err().contains("TOML"));
    // Through a file: refused, then replaced and added.
    let dir = std::env::temp_dir().join(format!("ohl-colours-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.ohlcolors");
    std::fs::write(&bad, "format = \"x\"\n").unwrap();
    let mut tool = ColorTool::new("t", None);
    tool.import_file(&bad);
    assert!(tool.status.as_ref().unwrap().1);
    let file = dir.join("mine.ohlcolors");
    p.write(&file).unwrap();
    let mut q = Palette {
        entries: vec![p.entries[0].clone()],
        mode: Default::default(),
    };
    tool.import_file(&file);
    tool.finish_import(&mut q, false);
    assert_eq!(q.entries, p.entries, "the one there not twice");
    let mut r = Palette {
        entries: vec![PaletteEntry {
            name: "x".into(),
            color: [9, 9, 9],
            description: String::new(),
        }],
        mode: Default::default(),
    };
    tool.import_file(&file);
    tool.finish_import(&mut r, true);
    assert_eq!(r.entries, p.entries);
    std::fs::remove_dir_all(&dir).ok();
}
