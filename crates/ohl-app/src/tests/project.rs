// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: weather cases shown, the project's libraries, project lines,
//! jumpers added, defaults in the tables.

use super::*;

/// View › Display weather cases: two cases ticked for every section
/// (the Sections table's display cases kept, not used); the strings are
/// drawn to their clamp in each; the wind from both sides draws each case
/// with wind twice.
#[test]
fn several_weather_cases_are_shown_for_the_whole_line() {
    let mut model = synthetic();
    // A section's own display case, set in the Sections table before.
    model.display_cases.insert("S1".into(), vec![0]);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let strings = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        crate::scene::layer(
            &r.model,
            crate::scene::Layer::Structures,
            r.case,
            &Selection::None,
        )
        .counts()
        .0
    };
    let one = strings(&mut harness);
    // No weather button on the quick bar any more: the View menu.
    assert!(harness.query_by_label_contains("Weather: ").is_none());
    // Only in the View menu.
    for menu in ["Sections", "Criteria"] {
        harness.get_by_label(menu).click();
        harness.run();
        assert!(
            harness.query_by_label("Display weather cases…").is_none(),
            "{menu}"
        );
    }
    harness.get_by_label("View").click();
    harness.run();
    harness
        .query_all_by_label("Display weather cases…")
        .last()
        .unwrap()
        .click();
    harness.run();
    harness.get_by_label("Display weather cases");
    harness.get_by_label("high wind").click();
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let wind = r
            .model
            .cases
            .iter()
            .position(|c| c.case.name == "high wind")
            .unwrap();
        assert_eq!(r.model.shown(r.case), vec![0, wind]);
        assert_eq!(r.model.section_cases("S2", 0), vec![0, wind]);
        // The Sections table is not changed, its case not used.
        assert_eq!(r.model.display_cases.get("S1"), Some(&vec![0]));
        assert_eq!(r.model.section_cases("S1", 0), vec![0, wind]);
        assert!(r.display_window.is_none());
        // Every section drawn twice.
        let n = r.model.line.sections.len();
        assert_eq!(r.model.shapes_shown(r.case).len(), 2 * n);
    }
    assert!(strings(&mut harness) > one, "strings in both cases");
    // The swung strings: in high wind a suspension clamp is not plumb.
    let r = harness.state_mut().ready().unwrap();
    let wind = r.model.shown(0)[1];
    let s1 = &r.model.cases[wind].shapes[0];
    let a = r
        .model
        .line
        .wire_point(&r.model.line.sections[0].points[1])
        .unwrap()
        .clone();
    let c = s1.clamps[1];
    assert!((c[0] - a.position[0]).hypot(c[1] - a.position[1]) > 0.3);
    // Each section's own case (Sections table): S1 in its case only.
    r.model.cases_shown = crate::model::CasesShown::PerSection;
    assert_eq!(r.model.section_cases("S1", 0), vec![0]);
    assert_eq!(r.model.section_cases("S2", 0), vec![0, wind]);
    r.model.cases_shown = crate::model::CasesShown::AllSections;
    let strings_of = |m: &model::Model| {
        crate::scene::layer(m, crate::scene::Layer::Structures, 0, &Selection::None)
            .counts()
            .0
    };
    let one_side = strings_of(&r.model);
    // Wind from both sides: the windy case solved twice, swung both ways.
    r.model.set_wind(crate::model::WindDisplay::Both);
    r.model.ensure_solved(wind);
    assert!(
        strings_of(&r.model) > one_side,
        "strings drawn to both sides' clamps"
    );
    let solved = &r.model.cases[wind];
    let mirror = solved.mirror.as_ref().expect("solved from the left too");
    assert_eq!(solved.case.wind_from, ohl_weather::WindSide::Right);
    assert_eq!(mirror.case.wind_from, ohl_weather::WindSide::Left);
    let side = |s: &crate::model::Solved| {
        let c = s.shapes[0].clamps[1];
        (c[0] - a.position[0], c[1] - a.position[1])
    };
    let (r1, l1) = (side(solved), side(mirror));
    assert!(
        r1.0 * l1.0 + r1.1 * l1.1 < 0.0,
        "opposite swings {r1:?} {l1:?}"
    );
    let n = r.model.line.sections.len();
    assert_eq!(r.model.shapes_shown(0).len(), 3 * n);
    // Back to one side: the mirror goes.
    r.model.set_wind(crate::model::WindDisplay::Left);
    r.model.ensure_solved(wind);
    assert!(r.model.cases[wind].mirror.is_none());
    assert_eq!(
        r.model.cases[wind].case.wind_from,
        ohl_weather::WindSide::Left
    );
}

/// Weather case files of earlier versions: the wind side is dropped.
#[test]
fn old_weather_files_lose_the_wind_side() {
    let old = "format = \"ohl-weather-cases\"\nversion = 1\n\n[[case]]\nname = \"w\"\ntemperature = \"15 °C\"\nwind_pressure = \"400 Pa\"\nwind_from = \"left\"\n";
    let new = crate::model::without_wind_side(old).unwrap();
    assert!(!new.contains("wind_from") && new.contains("wind_pressure = \"400 Pa\""));
    assert!(crate::model::without_wind_side(&new).is_none());
    ohl_weather::WeatherCases::parse(&new).unwrap();
}

/// The project keeps its libraries: opened the first time, the wire
/// library and weather case file it reads are copied into it; opened again
/// without any library given, it reads its own (and the weather case file
/// used last); a library file saved goes into it; Export writes them out.
#[test]
fn the_project_keeps_its_libraries() {
    let mut first = synthetic_project("libraries-kept");
    let file = first.sources.file.clone();
    assert!(first.library_dir.is_some());
    assert!(
        first
            .log
            .iter()
            .any(|l| l.contains("copied into the project")),
        "{:?}",
        first.log
    );
    let session = &first.editing.as_ref().unwrap().session;
    let cables = session.library_files("cables").unwrap();
    let weather = session.library_files("weather").unwrap();
    assert!(!cables.is_empty() && weather.len() == 1);
    let weather_name = weather[0].0.clone();
    // A file of the working copy changed and saved: into the project.
    let lib = first.sources.library.clone().unwrap();
    let edited = lib.join(&cables[0].0);
    let text = std::fs::read_to_string(&edited).unwrap();
    std::fs::write(&edited, format!("# edited\n{text}")).unwrap();
    assert!(first.store_libraries().unwrap() > cables.len());
    let export = std::env::temp_dir().join(format!("ohl-app-export-{}", std::process::id()));
    std::fs::remove_dir_all(&export).ok();
    let n = first.export_libraries(&export).unwrap();
    assert!(n > cables.len());
    assert!(export.join("weather").join(&weather_name).is_file());
    drop(first);
    // Opened again with nothing given: its own libraries.
    let again = model::load(
        Sources {
            file,
            point_budget: 1000,
            corridor: 60.0,
            user: Some("tester".into()),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    assert!(
        again
            .log
            .iter()
            .any(|l| l.contains("cables:") && l.contains("from the project"))
    );
    let w = again.sources.weather.clone().unwrap();
    assert_eq!(w.file_name().unwrap().to_string_lossy(), weather_name);
    assert!(again.cases.len() > 1);
    let lib = again.sources.library.clone().unwrap();
    let text = std::fs::read_to_string(lib.join(&cables[0].0)).unwrap();
    assert!(text.starts_with("# edited"), "the edit was kept");
    assert_solved_fully(&again);
}

/// Clearance rules, galloping, ambient cases, unit costs, poles and
/// structure models next to the project are copied into it when it opens
/// (poles and models from its structure folder, written next to the
/// types); then the windows read the project's copies, a file saved goes
/// into it, and each project line keeps its own.
#[test]
fn the_project_keeps_its_rules_costs_poles_and_models() {
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-more-libraries", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join("Structures")).unwrap();
    let beside = [
        ("rules", "clearance-rules.ohlr"),
        ("galloping", "galloping-example.ohlg"),
        ("ambient", "thermal-ambient.ohlta"),
        ("costs", "unit-costs.ohlcost"),
    ];
    for (_, name) in beside {
        std::fs::copy(ex.join(name), dir.join(name)).unwrap();
    }
    let structures = [
        ("structures", "type-02.ohls"),
        ("femodels", "type-02-model.ohlfe"),
        ("poles", "steel-pole-30m.ohlp"),
    ];
    for (_, name) in structures {
        let to = dir.join("Structures").join(name);
        std::fs::copy(ex.join("Structures").join(name), to).unwrap();
    }
    // Loads typed for placed structure T2 in the structure application
    // (kind `loads`: only `*.loads.csv`, not another CSV there).
    let typed = "case,load,at,label,transverse,longitudinal,vertical,source,\
                 evaluated_transverse,evaluated_longitudinal,evaluated_vertical,missing,note\n\
                 wind,1,point,A1,2500,0,0,typed,,,,,\n";
    let structures_dir = dir.join("Structures");
    std::fs::write(structures_dir.join("type-02.T2.loads.csv"), typed).unwrap();
    std::fs::write(structures_dir.join("notes.csv"), "a,b\n").unwrap();
    let file = dir.join("p.ohl");
    let line = ohl_line::Line::load(&ex.join("synthetic-line.ohl.json")).unwrap();
    ohl_session::Session::open(&file)
        .unwrap()
        .import(&line, "tester")
        .unwrap();
    let open = || {
        model::load(
            Sources {
                file: file.clone(),
                library: Some(root().join("libraries/cables")),
                weather: Some(ex.join("weather-cases.ohlwc")),
                point_budget: 1000,
                corridor: 60.0,
                user: Some("tester".into()),
                ..Sources::default()
            },
            &|_| {},
        )
        .unwrap()
    };
    let m = open();
    let session = &m.editing.as_ref().unwrap().session;
    for (kind, name) in beside.iter().chain(&structures) {
        let files = session.library_files(kind).unwrap();
        assert!(files.iter().any(|(n, _)| n == name), "{kind}: {files:?}");
    }
    let work = m.library_dir.clone().unwrap();
    for (kind, name) in beside {
        assert_eq!(m.project_files(kind), [work.join(kind).join(name)]);
    }
    // Next to the structure types that name them.
    for (_, name) in structures {
        assert!(work.join("structures").join(name).is_file(), "{name}");
    }
    let loads = session.library_files("loads").unwrap();
    assert_eq!(
        loads,
        [("type-02.T2.loads.csv".to_owned(), typed.to_owned())]
    );
    assert!(work.join("structures/type-02.T2.loads.csv").is_file());
    drop(m);
    // The files next to the project gone: the project's are read.
    for (_, name) in beside {
        std::fs::remove_file(dir.join(name)).unwrap();
    }
    std::fs::remove_dir_all(dir.join("Structures")).unwrap();
    let mut m = open();
    let work = m.library_dir.clone().unwrap();
    assert_eq!(
        m.project_files("costs"),
        [work.join("costs").join("unit-costs.ohlcost")]
    );
    assert!(
        work.join("structures")
            .join("steel-pole-30m.ohlp")
            .is_file()
    );
    // T2's typed loads come back from the project; T4's, saved in the
    // structure application, go in.
    assert_eq!(
        std::fs::read_to_string(work.join("structures/type-02.T2.loads.csv")).unwrap(),
        typed
    );
    std::fs::write(work.join("structures/type-02.T4.loads.csv"), typed).unwrap();
    m.store_libraries().unwrap();
    let session = &m.editing.as_ref().unwrap().session;
    assert_eq!(session.library_files("loads").unwrap().len(), 2);
    let r = crate::clearance::Clearances::new(&m, None);
    assert_eq!(r.files, [work.join("rules").join("clearance-rules.ohlr")]);
    assert!(matches!(r.rules, Some(Ok(_))), "the only file chosen");
    // A rule file saved in the working copy goes into the project.
    let rules = std::fs::read_to_string(&r.files[0]).unwrap();
    std::fs::write(work.join("rules").join("more.ohlr"), &rules).unwrap();
    m.store_libraries().unwrap();
    let session = &mut m.editing.as_mut().unwrap().session;
    assert_eq!(session.library_files("rules").unwrap().len(), 2);
    // Each project line keeps its own.
    session.create_line("tester", "phase 1", "main").unwrap();
    m.switch_line("phase 1").unwrap();
    let session = &mut m.editing.as_mut().unwrap().session;
    let changed = format!("# phase 1\n{rules}");
    session
        .put_library_file("tester", "rules", "more.ohlr", &changed)
        .unwrap();
    m.refresh_project_libraries().unwrap();
    let read = |m: &model::Model| {
        let w = m.library_dir.clone().unwrap();
        std::fs::read_to_string(w.join("rules").join("more.ohlr")).unwrap()
    };
    assert_eq!(read(&m), changed);
    m.switch_line("main").unwrap();
    assert_eq!(read(&m), rules, "main keeps its own");
    let session = &m.editing.as_ref().unwrap().session;
    let main = session.library_files("rules").unwrap();
    assert!(main.iter().all(|(_, t)| !t.starts_with("# phase 1")));
    std::fs::remove_dir_all(&dir).ok();
}

/// The section library (`.ohlsec`) in the `Sections/` folder next to the
/// project is copied into it when it opens; then the project's copy is
/// read (the folder gone), and a project without one has none.
#[test]
fn the_project_keeps_its_section_library() {
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-sections", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    let name = "example-sections.ohlsec";
    std::fs::copy(
        root().join("libraries/sections").join(name),
        dir.join("Sections").join(name),
    )
    .unwrap();
    let file = dir.join("p.ohl");
    let line = ohl_line::Line::load(&ex.join("synthetic-line.ohl.json")).unwrap();
    ohl_session::Session::open(&file)
        .unwrap()
        .import(&line, "tester")
        .unwrap();
    let open = || {
        model::load(
            Sources {
                file: file.clone(),
                library: Some(root().join("libraries/cables")),
                weather: Some(ex.join("weather-cases.ohlwc")),
                point_budget: 1000,
                corridor: 60.0,
                user: Some("tester".into()),
                ..Sources::default()
            },
            &|_| {},
        )
        .unwrap()
    };
    let m = open();
    let session = &m.editing.as_ref().unwrap().session;
    let files = session.library_files("sections").unwrap();
    assert!(files.iter().any(|(n, _)| n == name), "{files:?}");
    let work = m.library_dir.clone().unwrap();
    assert!(work.join("sections").join(name).is_file());
    assert!(m.section_library().unwrap().section("EA-61x7").is_some());
    drop(m);
    // The folder gone: the project's copy.
    std::fs::remove_dir_all(dir.join("Sections")).unwrap();
    let m = open();
    let lib = m.section_library().unwrap();
    assert!(lib.section("EA-61x7").is_some());
    let mut out = dir.join("export");
    m.export_libraries(&out).unwrap();
    out.push("sections");
    assert!(out.join(name).is_file(), "exported with the others");
    drop(m);
    // Another project with no section library next to it: none.
    let other = dir.join("q.ohl");
    ohl_session::Session::open(&other)
        .unwrap()
        .import(&line, "tester")
        .unwrap();
    let m = model::load(
        Sources {
            file: other,
            library: Some(root().join("libraries/cables")),
            point_budget: 1000,
            user: Some("tester".into()),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    let session = &m.editing.as_ref().unwrap().session;
    assert!(session.library_files("sections").unwrap().is_empty());
    assert!(m.section_library().unwrap().sections.is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

/// Lines: new lines started from the one worked on, shown next to it (two
/// ticked in a row: the menu stays open), worked on one at a time (each
/// its own undo), brought up to date from the main line, submitted for
/// review.
#[test]
fn project_lines_are_made_shown_worked_on_and_brought_up_to_date() {
    use crate::project_lines::LinesWindow;
    let model = synthetic_project("lines");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        // A twin bundle, to see it drawn in the other lines too.
        let r = harness.state_mut().ready().unwrap();
        let mut s = r.model.line.sections[0].clone();
        (s.bundle, s.bundle_spacing) = (2, Some(0.4));
        let (id, value) = (s.id.clone(), serde_json::to_value(&s).unwrap());
        r.perform(crate::edit::Action::Command {
            what: "twin bundle".into(),
            command: ohl_session::Command::Put {
                kind: ohl_session::Kind::Section,
                id,
                value,
                ord: None,
            },
        });
        assert_eq!(r.model.line.sections[0].bundle, 2, "{:?}", r.status);
    }
    // Two phases from the main line, through the menu.
    for phase in ["phase 1", "phase 2"] {
        harness.get_by_label("Versions").click();
        harness.run();
        harness.get_by_label("New line from main…").click();
        harness.run();
        {
            let r = harness.state_mut().ready().unwrap();
            let Some(LinesWindow::New { name, from, .. }) = &mut r.lines_window else {
                panic!("{:?}", r.lines_window);
            };
            assert_eq!(from, "main");
            *name = phase.into();
        }
        harness.run();
        harness.get_by_label("Create").click();
        harness.run();
        let r = harness.state_mut().ready().unwrap();
        assert!(r.lines_window.is_none(), "{:?}", r.status);
    }
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.project_line(), "main", "still on main");
        let names: Vec<String> = r
            .model
            .session()
            .unwrap()
            .lines()
            .unwrap()
            .into_iter()
            .map(|l| l.name)
            .collect();
        assert_eq!(names, ["main", "phase 1", "phase 2"]);
    }
    // Both shown, ticked in a row: the menu stays open.
    harness.get_by_label("Versions").click();
    harness.run();
    harness.get_by_label("phase 1").click();
    harness.run();
    harness.get_by_label("phase 2").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.other_lines.len(), 2);
        assert_ne!(r.model.other_lines[0].color, r.model.other_lines[1].color);
        // Drawn exactly as the line worked on: every structure, string,
        // subconductor, yoke and jumper.
        r.model.other_lines.truncate(1);
        let count = |m: &model::Model, l: crate::scene::Layer| {
            crate::scene::layer(m, l, 0, &Selection::None).counts().0
        };
        // Just started, it differs in nothing: nothing drawn of it.
        assert_eq!(count(&r.model, crate::scene::Layer::OtherLines), 0);
        // Whole lines asked for: drawn exactly as the line worked on (every
        // structure, string, subconductor, yoke and jumper) once solved in
        // the case shown (in the background; exit clause: a minute).
        let shown = vec![0usize];
        r.model.ensure_solved(0);
        let ctx = egui::Context::default();
        let start = std::time::Instant::now();
        while !r.model.other_lines[0].cases[0].ready {
            r.model.want_others(&shown, &ctx);
            r.model.poll_others();
            assert!(start.elapsed().as_secs() < 60, "not solved in a minute");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        r.model.other_lines_whole = true;
        let working = count(&r.model, crate::scene::Layer::Structures)
            + count(&r.model, crate::scene::Layer::Wires);
        assert_eq!(count(&r.model, crate::scene::Layer::OtherLines), working);
        r.model.other_lines_whole = false;
    }
    // Work on phase 1: T7 deleted there only; undone there.
    {
        let r = harness.state_mut().ready().unwrap();
        r.switch_line("phase 1");
        for k in 0..r.model.cases.len() {
            r.model.ensure_solved(k);
        }
    }
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.project_line(), "phase 1");
        assert!(r.model.other_lines.iter().all(|o| o.name != "phase 1"));
        assert_solved_fully(&r.model);
        r.perform(crate::edit::Action::Delete {
            kind: ohl_session::Kind::Structure,
            id: "T7".into(),
            confirm: true,
        });
        assert!(r.model.line.structure("T7").is_none(), "{:?}", r.status);
        let s = r.model.session().unwrap();
        assert!(s.line_named("main").unwrap().structure("T7").is_some());
    }
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.model.line.structure("T7").is_some(), "undone on phase 1");
        r.perform(crate::edit::Action::Delete {
            kind: ohl_session::Kind::Structure,
            id: "T7".into(),
            confirm: true,
        });
        // Main corrected meanwhile (T2 moved).
        r.switch_line("main");
        for k in 0..r.model.cases.len() {
            r.model.ensure_solved(k);
        }
        let p = r.model.line.structure("T2").unwrap().position;
        r.perform(crate::edit::Action::Command {
            what: "T2 east".into(),
            command: ohl_session::Command::MoveStructure {
                id: "T2".into(),
                position: [p[0] + 2.0, p[1], p[2]],
            },
        });
        r.switch_line("phase 1");
        for k in 0..r.model.cases.len() {
            r.model.ensure_solved(k);
        }
        assert_eq!(r.model.line.structure("T2").unwrap().position, p, "a copy");
        r.lines_window = Some(LinesWindow::BringIn {
            from: "main".into(),
            choices: Default::default(),
            plan: None,
        });
    }
    harness.run();
    assert!(harness.query_by_label_contains("No conflict").is_some());
    harness.get_by_label("Bring in").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let t2 = r.model.line.structure("T2").unwrap().position[0];
        let main_t2 = r
            .model
            .session()
            .unwrap()
            .line_named("main")
            .unwrap()
            .structure("T2")
            .unwrap()
            .position[0];
        assert_eq!(t2, main_t2, "main's correction brought in: {:?}", r.status);
        assert!(
            r.model.line.structure("T7").is_none(),
            "the phase's own change kept"
        );
        assert_solved_fully(&r.model);
        r.lines_window = Some(LinesWindow::Review {
            note: "phase 1 ready".into(),
        });
    }
    harness.run();
    harness.get_by_label("Submit").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let reviews = r.model.session().unwrap().reviews().unwrap();
    assert_eq!(reviews.len(), 1, "{:?}", r.status);
    assert_eq!(
        (reviews[0].branch.as_str(), reviews[0].target.as_str()),
        ("phase 1", "main")
    );
    // Remembered for this user: the project opens on phase 1 again.
    assert_eq!(r.model.view_prefs().line.as_deref(), Some("phase 1"));
}

/// Lines › Project lines: the window lists every line; rights shown.
#[test]
fn the_project_lines_window_lists_the_lines() {
    use crate::project_lines::LinesWindow;
    let model = synthetic_project("lines-window");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.project_op(
            "phase",
            ohl_server::Op::CreateLine {
                name: "phase 1".into(),
                from: "main".into(),
            },
        );
    }
    harness.get_by_label("Versions").click();
    harness.run();
    harness.get_by_label_contains("Project lines").hover();
    harness.run();
    harness.get_by_label("Project lines…").click();
    harness.run();
    assert!(harness.query_by_label("main line").is_some());
    assert!(harness.query_by_label("phase 1").is_some());
    // Rename phase 1, then delete it (back to the list in between).
    harness.state_mut().ready().unwrap().lines_window = Some(LinesWindow::Rename {
        from: "phase 1".into(),
        to: "phase A".into(),
    });
    harness.run();
    harness.get_by_label("Rename").click();
    harness.run();
    assert_eq!(
        harness.state_mut().ready().unwrap().lines_window,
        Some(LinesWindow::Manage)
    );
    harness.state_mut().ready().unwrap().lines_window = Some(LinesWindow::Delete("phase A".into()));
    harness.run();
    harness.get_by_label("Delete").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(
        r.model.session().unwrap().lines().unwrap().len(),
        1,
        "{:?}",
        r.status
    );
}

/// Add jumper: on a structure, the jumper missing between two section
/// ends of the same circuit and phase; between two strings right-clicked.
#[test]
fn jumpers_are_added_where_none_are() {
    use crate::tools::Tool;
    let model = synthetic_project("jumpers");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // S2 given S1's circuit and phase: their ends at T1 are to be joined.
    {
        let r = harness.state_mut().ready().unwrap();
        let s1 = r.model.line.sections[0].clone();
        let mut s2 = r.model.stored_line().unwrap().sections[1].clone();
        s2.circuit_name = s1.circuit_name.clone();
        s2.phase_label = s1.phase_label.clone();
        assert!(r.perform(crate::edit::Action::Command {
            what: "same phase".into(),
            command: ohl_session::Command::Put {
                kind: ohl_session::Kind::Section,
                id: s2.id.clone(),
                value: serde_json::to_value(&s2).unwrap(),
                ord: None,
            },
        }));
    }
    let n = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .jumpers
        .len();
    use_tool(
        &mut harness,
        Tool::AddJumper,
        Selection::Structure("T1".into()),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        // T1 and T5: both ends of S1 and S2 — at T1 here.
        assert_eq!(r.model.line.jumpers.len(), n + 1, "{:?}", r.status);
        let j = r.model.line.jumpers.last().unwrap();
        assert_eq!(j.from.structure(), Some("T1"));
        assert_eq!(j.to.structure(), Some("T1"));
        assert!(j.sag.is_some());
        assert_solved_fully(&r.model);
    }
    // Again: none missing.
    use_tool(
        &mut harness,
        Tool::AddJumper,
        Selection::Structure("T1".into()),
        None,
    );
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .model
            .line
            .jumpers
            .len(),
        n + 1
    );
    // Two strings: its window first, given by its length, the least
    // between its ends offered; added on Apply.
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 1), None);
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 11), None);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.line.jumpers.len(), n + 1, "nothing added yet");
        let Some(crate::editors::Form::Jumper(f)) = &r.editor else {
            panic!("its window: {:?}", r.status)
        };
        assert!(f.new);
        let (a, b) = (
            r.model
                .line
                .wire_point(&ohl_line::SectionPoint::new("T5", 1))
                .unwrap()
                .position,
            r.model
                .line
                .wire_point(&ohl_line::SectionPoint::new("T5", 11))
                .unwrap()
                .position,
        );
        let chord = (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt();
        let l = f.jumper.length.expect("given by its length");
        assert!(l >= chord && l < chord + 0.011, "{l} for {chord}");
        assert!(f.jumper.sag.is_none());
        // Its sag chosen instead: still the user's choice.
        if let Some(crate::editors::Form::Jumper(f)) = &mut r.editor {
            f.jumper.length = None;
            f.jumper.sag = Some(0.5);
        }
    }
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.line.jumpers.len(), n + 2, "{:?}", r.status);
    assert!(r.editor.is_none());
    assert!(matches!(r.selection, Selection::Jumper(_)));
}

/// A jumper between two different circuits: told first (Continue /
/// Cancel), then both compared and the one that stays chosen; the other's
/// sections take its name, phase and system with the jumper, undone
/// together.
#[test]
fn a_jumper_joining_two_circuits_asks_which_stays() {
    use crate::tools::Tool;
    let model = synthetic_project("circuit-join");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let n = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .jumpers
        .len();
    // S1 (L1) and S2 (L2) at T5: different phases.
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 1), None);
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 11), None);
    let apply = |harness: &mut Harness<'_, App>| {
        harness.run();
        harness.get_by_label("Apply").click();
        harness.run();
    };
    apply(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        let join = r.circuit_join.as_ref().expect("asked first");
        assert_eq!(join.step, crate::circuit_join::Step::Warn);
        assert_eq!(r.model.line.jumpers.len(), n, "nothing added yet");
    }
    harness.run();
    harness.get_by_label("You are trying to connect 2 different conductor paths");
    // Cancel: nothing changes.
    harness.get_by_label("Cancel").click();
    harness.run();
    assert!(harness.state_mut().ready().unwrap().circuit_join.is_none());
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 1), None);
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 11), None);
    apply(&mut harness);
    harness.get_by_label("Continue").click();
    harness.run();
    harness.get_by_label_contains("system's phases");
    // Keep the circuit at the jumper's other end (S2's phase).
    harness
        .state_mut()
        .ready()
        .unwrap()
        .circuit_join
        .as_mut()
        .unwrap()
        .keep = 1;
    harness.run();
    harness.get_by_label_contains("Join into").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.circuit_join.is_none());
        assert_eq!(r.model.line.jumpers.len(), n + 1, "{:?}", r.status);
        let line = &r.model.line;
        assert_eq!(line.sections[0].phase_label, line.sections[1].phase_label);
        assert_eq!(line.sections[0].phase_label, "L2");
    }
    // One undo takes both back.
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.line.jumpers.len(), n);
    assert_eq!(r.model.line.sections[0].phase_label, "L1");
}

#[test]
#[ignore = "writes images of the circuit join windows to OHL_SHOTS (a look by eye)"]
fn circuit_join_window_images() {
    use crate::tools::Tool;
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let model = synthetic_project("circuit-join-shots");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 1), None);
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 11), None);
    let shot = |h: &mut Harness<'_, App>, name: &str| {
        h.run();
        h.render()
            .unwrap()
            .save(format!("{dir}/{name}.png"))
            .unwrap();
    };
    shot(&mut harness, "circuit-join-1");
    harness.get_by_label("Continue").click();
    shot(&mut harness, "circuit-join-2");
}

/// ⌘S writes the project file through and says so; the hotbar is at the
/// bottom of the 3D view and goes with it when another page is in front.
#[test]
fn command_s_saves_and_the_hotbar_goes_with_the_3d_view() {
    let model = synthetic_project("save");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    press(&mut harness, egui::Key::S, egui::Modifiers::COMMAND);
    {
        let r = harness.state_mut().ready().unwrap();
        let (text, error) = r.status.clone().unwrap();
        assert!(text.starts_with("saved: project file"), "{text}");
        assert!(!error);
    }
    let hotbar_shown = |h: &Harness<'_, App>| {
        h.query_by_label("Select — right-click an object to select it · 1–9 or wheel (captured) · E: inventory (arrange)").is_some()
        || h.query_all_by_label_contains("E: inventory").next().is_some()
    };
    harness.state_mut().show_tab(crate::app::Tab::View3d);
    harness.run();
    let view = harness.state_mut().ready().unwrap().view3d.rect.unwrap();
    let bar = harness
        .ctx
        .memory(|m| m.area_rect(egui::Id::new("hotbar")))
        .unwrap();
    assert!(view.contains_rect(bar), "{bar:?} in {view:?}");
    assert!(hotbar_shown(&harness));
    // The plan in front: no hotbar.
    harness.state_mut().show_tab(crate::app::Tab::Plan);
    harness.run();
    harness.run();
    assert!(!hotbar_shown(&harness));
}

/// Parameters › Controls › 3D, Plan, Profile: a window each; the 3D one
/// chooses the navigation scheme (no longer in the Model page or written
/// over the 3D view).
#[test]
fn parameters_show_the_controls_of_each_view() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    assert!(harness.query_by_label("Controls (3D)").is_none());
    for (page, line) in [
        ("3D", "Navigation scheme"),
        ("Plan", "zoom at the pointer"),
        ("Profile", "move along the route"),
    ] {
        harness.get_by_label("Help").click();
        harness.run();
        harness.get_by_label_contains("Controls").click();
        harness.run();
        harness.get_by_label(page).click();
        harness.run();
        harness.get_by_label(&format!("Controls: {page}"));
        harness.get_by_label_contains(line);
        harness.state_mut().ready().unwrap().controls_window = None;
        harness.run();
    }
}

/// Going to an object (from the 3D view, a table, a failure) before the
/// plan was ever shown centred it without a scale: shown later, it was
/// empty (every object on one point). It fits its scale, keeping that
/// centre.
#[test]
fn the_plan_shows_the_line_after_a_go_to_before_it_was_shown() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().show_tab(crate::app::Tab::View3d);
    harness.run();
    harness
        .state_mut()
        .ready()
        .unwrap()
        .go_to(Selection::Structure("T3".into()));
    harness.state_mut().show_tab(crate::app::Tab::Plan);
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.plan.scale() > 0.0, "{}", r.plan.scale());
    // Zoomed in on the line, not out to a point.
    let rect = r.plan.drawn.unwrap();
    assert!(r.plan.scale() * 100.0 < f64::from(rect.width()));
}

/// Tables show the values used where an object leaves them to a default
/// (a jumper's bundle, EI and clamp angles; a section's system and display
/// cases), marked as defaults; Apply leaves them unset unless typed in.
#[test]
fn tables_show_defaults_and_leave_them_unset() {
    use crate::tables::TableKind;
    let model = synthetic_project("defaults");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (key, ei) = {
        let r = harness.state_mut().ready().unwrap();
        let j = r.model.line.jumpers[0].clone();
        assert!(j.bending_stiffness.is_none() && j.angle_from_deg.is_none());
        let rows = TableKind::Jumpers.rows(&r.model, r.case);
        let row = rows.iter().find(|x| x.key == j.id).unwrap();
        let c = |name| TableKind::Jumpers.column(name);
        let cable = r.model.library.get(&j.cable).unwrap();
        let ei = ohl_line::jumper::estimated_bending_stiffness(
            cable,
            ohl_line::jumper::DEFAULT_STIFFNESS_FACTOR,
        );
        assert_eq!(row.cells[c("EI N·m²")], format!("{ei:.1}"));
        assert_eq!(row.cells[c("EI factor")], "0.20");
        assert!(row.defaulted[c("EI factor")]);
        assert_eq!(row.cells[c("angle from °")], "30.0");
        assert_eq!(row.cells[c("bundle")], "1");
        assert!(row.defaulted[c("EI N·m²")] && row.defaulted[c("angle to °")]);
        let sections = TableKind::Sections.rows(&r.model, r.case);
        let s = TableKind::Sections.column("display cases");
        assert!(!sections[0].cells[s].is_empty() && sections[0].defaulted[s]);
        (j.id, ei)
    };
    // Another value typed in: the defaults stay unset.
    set_cell(&mut harness, TableKind::Jumpers, &key, "temp °C", "20");
    apply_table(&mut harness, TableKind::Jumpers, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let j = r.model.line.jumpers.iter().find(|j| j.id == key).unwrap();
        assert_eq!(j.temperature, 20.0);
        assert!(j.bending_stiffness.is_none() && j.angle_from_deg.is_none() && j.bundle.is_none());
    }
    // A default typed over is set.
    set_cell(&mut harness, TableKind::Jumpers, &key, "angle to °", "10");
    apply_table(&mut harness, TableKind::Jumpers, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let j = r.model.line.jumpers.iter().find(|j| j.id == key).unwrap();
    assert_eq!(j.angle_to_deg, Some(10.0));
    assert!(j.bending_stiffness.is_none() && ei > 0.0);
}
