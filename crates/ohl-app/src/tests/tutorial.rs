// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The tutorial "Start from scratch — your first line" (docs/tutorial/),
//! done chapter by chapter through the UI, as far as the harness can: a
//! project from the starter template, the ground from its survey, the
//! route and the alignment, six structures, the strings their types
//! bring, six sections strung, the jumpers at the angle structure, the
//! results, the checks, the project saved and opened again. Each step
//! names its chapter:
//! a UI change that breaks the tutorial fails here. With `OHL_RENDER_PNG`
//! set to a folder (and a GPU) a few windows are saved there as images
//! (the tutorial's, docs/images/tutorial/).
//!
//! What the harness cannot do is done through the same function the UI
//! calls, said where: the native file dialogs (rfd), the right-clicks in
//! the views (the tool is used on what the click would give, as the other
//! tests do) and the dropdowns of the windows (their field set).

use super::*;

use crate::editors::Form;
use crate::tools::Tool;

/// The alignment points of the tutorial (chapter 2): E, N in metres, on
/// the starter's invented ground (each on a survey point); an angle at
/// the fourth.
pub(crate) const POINTS: [(f64, f64); 6] = [
    (0.0, 0.0),
    (200.0, 0.0),
    (400.0, 0.0),
    (600.0, 0.0),
    (800.0, 100.0),
    (1000.0, 200.0),
];

/// The tension structures (chapter 3): at the ends and the angle.
const TENSION: [usize; 3] = [0, 3, 5];

/// The two sections of each phase (chapter 5): their structures in line
/// order, from the first tension structure's set ahead (40 + phase) over
/// the suspension structures' (phase) to the last one's set behind (20 +
/// phase).
const SECTIONS: [&[&str]; 2] = [&["T1", "T2", "T3", "T4"], &["T4", "T5", "T6"]];

/// The starter's structure types (chapter 3).
const TENSION_TYPE: &str = "starter-tension";
const SUSPENSION: &str = "starter-suspension";

/// Saves the window as `name`.png in `OHL_RENDER_PNG` (when set and a GPU
/// renders).
fn shot(h: &mut Harness<'_, App>, name: &str) {
    if ohl_core::env::var_os("OHL_RENDER_PNG").is_some() {
        h.run();
        crate::ux_tests::shot(h, &format!("tutorial-{name}"));
    }
}

/// Clicks `label` (the only one of that name).
fn click(h: &mut Harness<'_, App>, label: &str) {
    h.get_by_label(label).click();
    h.run();
}

/// Waits (up to two minutes) until `done`.
fn wait(h: &mut Harness<'_, App>, what: &str, mut done: impl FnMut(&mut App) -> bool) {
    let started = std::time::Instant::now();
    while !done(h.state_mut()) {
        assert!(
            started.elapsed().as_secs() < 120,
            "{what}: not done in 120 s"
        );
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    h.run();
}

/// Run in the Vertical clearances window (open): the spans found below
/// a rule.
fn vertical(h: &mut Harness<'_, App>) -> usize {
    run_in(h, "Vertical clearances").click();
    h.step();
    wait(h, "the vertical clearances", |a| {
        matches!(
            a.ready().unwrap().vertical_clearances.result(),
            (Some(_), false)
        )
    });
    let r = h.state_mut().ready().unwrap();
    let (Some(rep), _) = r.vertical_clearances.result() else {
        unreachable!("waited for");
    };
    let rep = rep.as_ref().unwrap();
    assert!(!rep.states.is_empty());
    rep.zone.findings.len()
}

#[test]
fn the_tutorial_builds_a_first_line() {
    use crate::files::{FileAction, LOCAL_CRS, Template};
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    let dir = std::env::temp_dir().join(format!("ohl-tutorial-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let user = crate::files::user();

    // Chapter 1: File › New project… › Starter template (for learning),
    // local coordinates; "Choose where to save it and create…" asks for
    // the file in a native dialog: the project is made with the function
    // the window calls, then opened as the window opens it (File › Open).
    let mut np = crate::files::NewProject::starter();
    assert_eq!(np.template, Template::Starter);
    assert_eq!(np.crs().unwrap(), LOCAL_CRS);
    np.name = "My first line".into();
    let path = crate::files::create_project_from(
        &dir.join("My first line"),
        &np.name,
        &np.crs().unwrap(),
        &user,
        np.template,
    )
    .unwrap();
    let template = Sources {
        point_budget: 1000,
        corridor: 60.0,
        user: Some(user.clone()),
        ..Sources::default()
    };
    let mut builder = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .with_max_steps(64);
    if gpu {
        builder = builder.wgpu();
    }
    let mut h = builder.build_eframe(move |cc| App::empty(template, cc.wgpu_render_state.clone()));
    h.run();
    h.state_mut()
        .file_action_for_test(FileAction::Open(Some(path.clone())));
    wait(&mut h, "opening the project", |a| {
        a.ready().is_some() || a.failed().is_some()
    });
    assert_eq!(h.state_mut().failed(), None);
    // The first steps window asks for the route: Later (chapter 2).
    h.get_by_label("New project: first steps");
    click(&mut h, "Later");
    {
        let r = h.state_mut().ready().unwrap();
        assert!(r.first_steps.is_none());
        let line = &r.model.line;
        assert!(line.routes.is_empty() && line.structures.is_empty());
        assert_eq!(r.model.types.as_ref().map(|(t, _)| t.len()), Some(2));
        assert_eq!(r.model.cases.len(), 5);
        assert!(r.model.ground.is_none());
    }
    // The ground: Terrain › Terrain surfaces (TIN)… › Import… — a native
    // file dialog: the survey beside the project imported as the button
    // does (surfaces::Action::Import).
    crate::tests::line_menu(&mut h, "Terrain");
    click(&mut h, "Terrain surfaces (TIN)…");
    h.get_by_label("Terrain surfaces (TIN)");
    h.get_by_label("Import…");
    {
        let ctx = egui::Context::default();
        let app = h.state_mut();
        let mut plugins = std::mem::take(&mut app.plugins);
        let r = app.ready().unwrap();
        let project = r.model.sources.file.clone();
        r.surfaces.act(
            crate::surfaces::Action::Import(dir.join(crate::starter::SURVEY)),
            &mut r.model,
            &mut plugins,
            &user,
            &project,
            &ctx,
        );
        app.plugins = plugins;
    }
    wait(&mut h, "the ground", |a| {
        a.ready().unwrap().model.ground_source == "surface starter-survey"
    });
    assert!(h.state_mut().ready().unwrap().model.ground.is_some());
    h.state_mut().ready().unwrap().surfaces.open = false;
    h.run();

    // Chapter 2: the route (Next steps › Name the route… opens the first
    // steps again), then the alignment with New alignment.
    h.state_mut().show_tab(crate::app::Tab::NextSteps);
    h.run();
    click(&mut h, "Next: Name the route…");
    h.get_by_label("New project: first steps");
    click(&mut h, "Create the route");
    {
        let r = h.state_mut().ready().unwrap();
        assert_eq!(r.model.line.routes.len(), 1, "{:?}", r.status);
        assert_eq!(r.model.line.routes[0].id, "route 1");
        assert_eq!(r.hotbar.tool(), Tool::NewAlignment);
    }
    // A right-click on the ground at each point (in the plan view): no
    // point to snap to, so the coordinates window, E and N typed (here
    // given as the click), the ground's height; Add the point.
    for (x, y) in POINTS {
        let z = h
            .state_mut()
            .ready()
            .unwrap()
            .model
            .ground_at(x, y)
            .unwrap();
        use_tool(&mut h, Tool::NewAlignment, Selection::None, Some([x, y, z]));
        h.get_by_label("Point coordinates");
        click(&mut h, "Add the point");
        assert!(h.state_mut().ready().unwrap().coord_dialog.is_none());
    }
    // Enter: its route (the only one), Make the alignment.
    press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    h.get_by_label("New alignment: its route");
    click(&mut h, "Make the alignment");
    let points: Vec<(String, [f64; 3])> = {
        let r = h.state_mut().ready().unwrap();
        let line = &r.model.line;
        assert_eq!(line.alignments.len(), 1, "{:?}", r.status);
        assert_eq!(line.alignments[0].route, "route 1");
        line.points_of(&line.alignments[0])
            .iter()
            .map(|p| (p.id.clone(), p.position))
            .collect()
    };
    assert_eq!(points.len(), POINTS.len());
    for ((_, p), (x, y)) in points.iter().zip(POINTS) {
        assert!((p[0] - x).abs() < 1e-3 && (p[1] - y).abs() < 1e-3, "{p:?}");
    }
    // The plan in front (chapter 2, step 3).
    h.state_mut().show_tab(crate::app::Tab::Plan);
    h.run();

    // Chapter 3: Add structure, a right-click on the first alignment
    // point: the strain type there; Also on other points of this
    // alignment: All, the suspension type at the second and fourth (their
    // type dropdowns: the field set), the strain type above at the others.
    use_tool(
        &mut h,
        Tool::AddStructure,
        Selection::AlignmentPoint {
            id: points[0].0.clone(),
        },
        Some(points[0].1),
    );
    h.get_by_label("Add structure");
    {
        let r = h.state_mut().ready().unwrap();
        let Some(Form::Structure(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.id, "T1");
        assert!(f.on_alignment.is_some() && f.along_line);
        // The type: its button opens the structure picker (Choose); the
        // field set as Choose sets it.
        f.structure_type = Some(TENSION_TYPE.into());
    }
    h.run();
    h.get_by_label_contains("Also on other points of this alignment")
        .click();
    h.run();
    click(&mut h, "All");
    {
        let r = h.state_mut().ready().unwrap();
        let Some(Form::Structure(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        let more = f.more.as_mut().expect("the other points");
        assert_eq!(more.rows.len(), POINTS.len() - 1);
        assert!(more.rows.iter().all(|r| r.on));
        for row in &mut more.rows {
            if !TENSION.contains(&row.index) {
                row.structure_type = Some(SUSPENSION.into());
            }
        }
    }
    h.run();
    shot(&mut h, "add-structure");
    click(&mut h, "Apply");
    let ids: Vec<String> = {
        let r = h.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let line = &r.model.line;
        assert_eq!(line.structures.len(), POINTS.len());
        line.points_of(&line.alignments[0])
            .iter()
            .map(|p| p.structure.clone().expect("a structure on each point"))
            .collect()
    };
    assert_eq!(ids, ["T1", "T2", "T3", "T4", "T5", "T6"]);
    {
        let r = h.state_mut().ready().unwrap();
        let line = &r.model.line;
        for (k, id) in ids.iter().enumerate() {
            let s = line.structure(id).unwrap();
            let tension = TENSION.contains(&k);
            let want = if tension { TENSION_TYPE } else { SUSPENSION };
            assert_eq!(s.structure_type.as_deref(), Some(want), "{id}");
            // Chapter 4: the strings come with the type — four at a
            // suspension structure (three phases, the earth wire), two at
            // each place of a strain structure (eight).
            assert_eq!(s.strings.len(), if tension { 8 } else { 4 }, "{id}");
        }
    }

    // Chapter 4: a string's window (Edit, a right-click on it): the set
    // and its values; Cancel.
    use_tool(&mut h, Tool::Edit, string_at("T2", 1), None);
    h.get_by_label("Strings of set 1 of T2");
    // Cancel (here Escape, the same: the window opens where the pointer
    // was, partly below the harness's screen).
    h.get_by_label("Cancel");
    press(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    {
        let r = h.state_mut().ready().unwrap();
        assert!(
            r.editor.is_none(),
            "{:?} {:?}",
            r.editor.as_ref().map(|f| f.title()),
            r.status
        );
    }

    // Chapter 5: String wire — the strings in line order (right-clicks),
    // Enter, the section's window: the circuit and the phase typed (the
    // fields set), Apply. Each phase in two sections: T1 to T4 and T4 to
    // T6 (the tension structures' sets 41-43 ahead, 21-23 behind).
    for phase in 1..=3u32 {
        for (k, structures) in SECTIONS.iter().enumerate() {
            let last = structures.len() - 1;
            for (i, structure) in structures.iter().enumerate() {
                let set = match i {
                    0 => 40 + phase,
                    i if i == last => 20 + phase,
                    _ => phase,
                };
                use_tool(&mut h, Tool::StringWire, string_at(structure, set), None);
            }
            assert_eq!(h.state_mut().ready().unwrap().chain.len(), structures.len());
            press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
            h.get_by_label("String wire: new section");
            {
                let r = h.state_mut().ready().unwrap();
                let Some(Form::Section(f)) = &mut r.editor else {
                    panic!("{:?}", r.editor);
                };
                assert!(f.new);
                assert_eq!(f.cable, "acsr-drake-26-7");
                // After the first: the window proposes the circuit and the
                // phase — the same phase across a tension structure, the
                // next one at a new wire point.
                if (phase, k) != (1, 0) {
                    assert_eq!(
                        (f.circuit.as_str(), f.phase_label.clone()),
                        ("C1", format!("L{phase}"))
                    );
                }
                f.circuit = "C1".into();
                f.phase_label = format!("L{phase}");
            }
            h.run();
            if phase == 1 && k == 0 {
                shot(&mut h, "string-wire");
            }
            // Apply (here Enter, the same: the window opens where the
            // pointer was, partly below the harness's screen).
            h.get_by_label("Apply");
            press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
            let r = h.state_mut().ready().unwrap();
            assert!(r.editor.is_none(), "{:?}", r.status);
        }
    }
    {
        let r = h.state_mut().ready().unwrap();
        assert_eq!(r.model.line.sections.len(), 6);
        // Every weather case solved, every section in each.
        for k in 0..r.model.cases.len() {
            r.model.ensure_solved(k);
        }
        for c in &r.model.cases {
            assert!(c.failed.is_empty(), "{}: {:?}", c.case.name, c.failed);
            assert_eq!(c.shapes.len(), 6, "{}", c.case.name);
        }
    }

    // Chapter 6: Add jumper, a right-click on T4: the three jumpers
    // missing there (same circuit and phase).
    use_tool(
        &mut h,
        Tool::AddJumper,
        Selection::Structure("T4".into()),
        None,
    );
    {
        let r = h.state_mut().ready().unwrap();
        assert_eq!(r.model.line.jumpers.len(), 3, "{:?}", r.status);
        // None missing at T1 (one section end per phase).
    }
    use_tool(
        &mut h,
        Tool::AddJumper,
        Selection::Structure("T1".into()),
        None,
    );
    {
        let r = h.state_mut().ready().unwrap();
        assert_eq!(r.model.line.jumpers.len(), 3);
        assert!(
            r.status
                .as_ref()
                .is_some_and(|(s, _)| s.contains("none missing at T1")),
            "{:?}",
            r.status
        );
        for k in 0..r.model.cases.len() {
            r.model.ensure_solved(k);
        }
        for c in &r.model.cases {
            assert!(c.failed.is_empty(), "{}: {:?}", c.case.name, c.failed);
            assert_eq!(c.jumpers.len(), 3, "{}", c.case.name);
        }
    }

    // Chapter 7: the results — every span sags, more at the maximum
    // temperature than in the everyday case; the views fitted.
    {
        let r = h.state_mut().ready().unwrap();
        let sag = |name: &str| -> f64 {
            let c = r.model.cases.iter().find(|c| c.case.name == name).unwrap();
            c.shapes
                .iter()
                .flat_map(|s| s.spans.iter().map(|x| x.sag))
                .fold(0.0, f64::max)
        };
        let (everyday, hot) = (sag("everyday"), sag("max temperature"));
        assert!(everyday > 1.0 && hot > everyday, "{everyday} {hot}");
    }
    h.state_mut().show_tab(crate::app::Tab::View3d);
    h.run();
    shot(&mut h, "line-3d");

    // Chapter 8: Checks › Wire clearances › Check wire clearances, with
    // the starter's rule file.
    click(&mut h, "Checks");
    h.get_by_label_contains("Wire clearances").hover();
    h.run();
    h.query_all_by_label("Check wire clearances")
        .last()
        .unwrap()
        .click();
    wait(&mut h, "the wire clearances", |a| {
        let r = a.ready().unwrap();
        r.clearances.report.is_some() && !r.clearances.running()
    });
    {
        let r = h.state_mut().ready().unwrap();
        assert!(
            r.clearances
                .files
                .iter()
                .any(|f| f.to_string_lossy().contains("starter-clearance-rules")),
            "{:?}",
            r.clearances.files
        );
        let rep = r.clearances.report.clone().unwrap().unwrap();
        // Every weather case of the rules (2026-10-28, not only the case
        // shown): the ground rule, the one between the wires and the
        // maximum temperature's.
        assert_eq!(rep.rules.len(), 3, "{:?}", rep.notes);
        assert!(rep.findings.is_empty(), "{:?}", rep.rules);
    }
    // Checks › Ground clearances › Vertical clearances…: the first rule's
    // distance set (7 m, as the starter's rule file; typed in its field),
    // Run.
    // The Clearances window closed (its ×), then the next window.
    h.state_mut().ready().unwrap().clearances.open = false;
    h.run();
    open_clearance_window(&mut h, "Vertical clearances…");
    {
        let r = h.state_mut().ready().unwrap();
        let w = &mut r.vertical_clearances;
        assert!(w.open);
        assert_eq!(w.rules, vec![crate::clearance_zones::first_rule()]);
        w.rules[0].distance = ohl_clearance::Requirement::Fixed(7.0);
    }
    h.run();
    // Every case, every span clear (the lowest at the maximum
    // temperature, about 9.7 m).
    assert_eq!(vertical(&mut h), 0);
    // S1 at 10 kN (Edit on a span of S1, its horizontal tension typed —
    // the field set —, Apply): too low at the maximum temperature; Run
    // again. Undo (⌘Z) brings the 20 kN back.
    use_tool(
        &mut h,
        Tool::Edit,
        Selection::Span {
            section: "S1".into(),
            span: 0,
        },
        None,
    );
    {
        let r = h.state_mut().ready().unwrap();
        let Some(Form::Section(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(
            f.stringing,
            ohl_line::Stringing::Tension {
                horizontal_tension: 20_000.0,
                temperature: 20.0
            }
        );
        f.stringing = ohl_line::Stringing::Tension {
            horizontal_tension: 10_000.0,
            temperature: 20.0,
        };
    }
    h.run();
    press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    let low = vertical(&mut h);
    assert!(low > 0);
    {
        let r = h.state_mut().ready().unwrap();
        let (Some(Ok(rep)), _) = r.vertical_clearances.result() else {
            unreachable!("waited for");
        };
        assert!(
            rep.zone.findings.iter().all(|f| f.section == "S1"),
            "{:?}",
            rep.zone.findings
        );
    }
    shot(&mut h, "vertical-clearances");
    press(&mut h, egui::Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(vertical(&mut h), 0);
    h.state_mut().ready().unwrap().vertical_clearances.open = false;
    h.run();
    // Checks › Wire tensions…: Add rule, its factor of RTS typed (0.5,
    // invented; the field set and kept as a change of it is), Run: every
    // span of every case listed.
    click(&mut h, "Checks");
    click(&mut h, "Wire tensions…");
    assert!(
        h.state_mut()
            .ready()
            .unwrap()
            .wire_tensions
            .rules
            .is_empty()
    );
    click(&mut h, "Add rule");
    {
        let r = h.state_mut().ready().unwrap();
        let w = &mut r.wire_tensions;
        assert!(w.open);
        assert_eq!(w.rules.len(), 1);
        assert!(w.why_not_run().is_some(), "the factor to set");
        w.rules[0].factor = 0.5;
        w.save(&mut r.model);
    }
    h.run();
    run_in(&h, "Wire tensions").click();
    wait(&mut h, "the wire tensions", |a| {
        a.ready().unwrap().wire_tensions.job.result.is_some()
    });
    {
        let r = h.state_mut().ready().unwrap();
        let Some(Ok(rep)) = &r.wire_tensions.job.result else {
            panic!(
                "{:?}",
                r.wire_tensions.job.result.as_ref().map(|r| r.is_ok())
            );
        };
        // Three phases of five spans, in every case; none above half the
        // RTS.
        assert_eq!(rep.report.rows.len(), 3 * 5 * r.model.cases.len());
        assert_eq!(rep.report.failures().count(), 0);
        r.wire_tensions.open = false;
    }
    h.run();
    // Checks › String checks…: the window's first rule (the strings' own
    // limits and uplift), Run.
    click(&mut h, "Checks");
    click(&mut h, "String checks…");
    assert!(h.state_mut().ready().unwrap().string_checks.open);
    run_in(&h, "String checks").click();
    wait(&mut h, "the string checks", |a| {
        a.ready().unwrap().string_checks.job.result.is_some()
    });
    {
        let r = h.state_mut().ready().unwrap();
        let Some(Ok(rep)) = &r.string_checks.job.result else {
            panic!("no string checks");
        };
        // The uplift checked; the starter's strings have no limits of
        // their own: said in the notes.
        assert!(!rep.report.rows.is_empty());
        assert_eq!(rep.report.failures().count(), 0);
        assert_eq!(rep.report.notes.len(), 2, "{:?}", rep.report.notes);
        r.string_checks.open = false;
    }
    h.run();

    // Chapter 9: Criteria › Loads for structures: everyday and high wind
    // ticked (the menu stays open), as strung; the line results: everyday
    // once, high wind from the left and from the right.
    crate::tests::line_menu(&mut h, "Criteria");
    h.get_by_label_contains("Loads for structures").hover();
    h.run();
    h.get_by_label("Weather cases sent");
    let tick = egui::accesskit::Role::CheckBox;
    h.get_by_role_and_label(tick, "everyday").click();
    h.run();
    h.get_by_role_and_label(tick, "high wind").click();
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        assert_eq!(
            r.model.line.structure_loads.weather_cases,
            ["everyday", "high wind"]
        );
        assert_eq!(r.model.line.structure_loads.condition, None);
        let loads = crate::structure_app::LineInputs::of(&r.model)
            .compute()
            .unwrap();
        let names: Vec<&str> = loads.weather.iter().map(|c| c.case.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "everyday",
                "high wind — wind from the left",
                "high wind — wind from the right"
            ]
        );
    }
    press(&mut h, egui::Key::Escape, egui::Modifiers::NONE);

    // Chapter 10: File › Save all (⌘S), File › Close, opened again (File
    // › Open recent: the same action): the line as built.
    press(&mut h, egui::Key::S, egui::Modifiers::COMMAND);
    h.state_mut().file_action_for_test(FileAction::Close);
    h.run();
    assert!(h.state_mut().ready().is_none());
    h.state_mut()
        .file_action_for_test(FileAction::Open(Some(path.clone())));
    wait(&mut h, "opening the project again", |a| {
        a.ready().is_some() || a.failed().is_some()
    });
    {
        let r = h.state_mut().ready().unwrap();
        assert!(r.first_steps.is_none());
        let line = &r.model.line;
        assert_eq!(line.structures.len(), POINTS.len());
        assert_eq!(line.sections.len(), 6);
        assert_eq!(line.jumpers.len(), 3);
        assert!(r.model.ground.is_some());
    }
    drop(h);
    std::fs::remove_dir_all(&dir).ok();
}
