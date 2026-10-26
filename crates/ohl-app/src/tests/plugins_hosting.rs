// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: calculation choices, cable models, plugins, the Python console,
//! hosted projects, structure loads, surfaces.

use super::*;

/// Sections are calculated as chosen in the Sections table (8 choices:
/// Initial, Creep, Load, Max Sag; RS or FE); Criteria › Creep-Stretch
/// names the weather cases for after creep and after load, as one command
/// (undone with ⌘Z), and every section is solved again.
#[test]
fn sections_are_calculated_after_creep_or_load_as_chosen() {
    use crate::tables::TableKind;
    use ohl_line::condition::{Calculation, CreepStretch};
    let model = synthetic_project("calculation");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let kind = TableKind::Sections;
    // The column's choices.
    {
        let r = harness.state_mut().ready().unwrap();
        let rows = kind.rows(&r.model, r.case);
        assert_eq!(rows[0].cells[kind.column("calculation")], "Initial FE");
    }
    set_cell(&mut harness, kind, "S1", "calculation", "Creep RS");
    apply_table(&mut harness, kind, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.line.sections[0].calculation, Calculation::CREEP_RS);
        // No creep weather case yet: S1 fails, saying where to choose one.
        let failed = &r.model.cases[0].failed;
        assert!(
            failed
                .iter()
                .any(|f| f.starts_with("S1") && f.contains("Creep-Stretch")),
            "{failed:?}"
        );
        // Criteria › Creep-Stretch: OK.
        r.creep_stretch_window = Some(CreepStretch {
            creep_case: Some("everyday".into()),
            load_cases: vec!["glaze ice".into(), "rime ice + wind".into()],
        });
    }
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(
            r.model.line.creep_stretch.creep_case.as_deref(),
            Some("everyday")
        );
        assert_eq!(r.model.line.condition_cases.loads.len(), 2);
        assert!(
            r.model.cases.iter().all(|c| c.failed.is_empty()),
            "solved again"
        );
        // A linear cable without creep data: after creep sags as initial.
        assert_solved_fully(&r.model);
    }
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.creep_stretch.is_empty(), "undone");
    assert!(
        !r.model.cases[0].failed.is_empty(),
        "S1 needs its case again"
    );
}

/// The wire library takes the cable models and, for bimetallic cables,
/// the curves of the outer and the core materials.
#[test]
fn cable_models_and_bimetallic_curves_are_read() {
    let text = r#"
format = "ohl-wire"
version = 1

[[cable]]
id = "bimetallic"
name = "Aluminium on steel"
kind = "acsr"
diameter = "28 mm"
mass_per_length = "1.6 kg/m"
rated_tensile_strength = "140 kN"
model = "nonlinear"

[cable.outer]
area = "400 mm²"
modulus = "55 GPa"
thermal_expansion = 23.0e-6
stress_strain = { stress_unit = "MPa", basis = "total", initial = [0.0, 250.0, -110.0, 40.0, -8.0], creep = [0.0, 160.0, -60.0, 15.0, -2.0], final_modulus = 550.0, max_compression = 0.0, reference_temperature = "20 °C" }

[cable.core]
area = "65 mm²"
modulus = "190 GPa"
thermal_expansion = 11.5e-6
stress_strain = { stress_unit = "MPa", basis = "total", initial = [0.0, 1800.0, -120.0, 30.0, -5.0], final_modulus = 1900.0, reference_temperature = "20 °C" }

[[cable]]
id = "linear-shift"
name = "Linear, creep as +15 °C"
kind = "aaac"
diameter = "30 mm"
mass_per_length = "1.5 kg/m"
rated_tensile_strength = "160 kN"
model = "linear_creep_temperature"
creep_temperature_shift = 15.0
outer = { area = "550 mm²", modulus = "55 GPa", thermal_expansion = 23e-6 }
"#;
    crate::library::LibraryKind::Cables
        .check(std::path::Path::new("x.ohlw"), text)
        .unwrap();
    let mut lib = ohl_cables::Library::default();
    lib.add_toml(std::path::Path::new("x.ohlw"), text).unwrap();
    let b = lib.get("bimetallic").unwrap();
    assert!(b.outer.stress_strain.is_some() && b.core.as_ref().unwrap().stress_strain.is_some());
    assert_eq!(
        lib.get("linear-shift").unwrap().model,
        ohl_cables::CableModel::LinearCreepTemperature
    );
    // A linear model without its shift is refused.
    let bad = text.replace("creep_temperature_shift = 15.0\n", "");
    assert!(
        crate::library::LibraryKind::Cables
            .check(std::path::Path::new("x.ohlw"), &bad)
            .is_err()
    );
}

/// The wire library page: the model is a list at the top (the fields it
/// needs appear, not removable), with the Bimetallic tick.
#[test]
fn the_cable_model_is_a_list_and_adds_its_fields() {
    use crate::library::LibraryKind;
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().ready().unwrap().open_page =
        Some((crate::app::Tab::Library(LibraryKind::Cables), None));
    harness.run();
    harness.run();
    // The example's first cable: ACSR, a core.
    let bimetallic = "Bimetallic conductor (outer strands and a core)";
    harness.get_by_label(bimetallic);
    // The list shows the model chosen (nonlinear by default).
    harness
        .get_by_value(crate::library::CABLE_MODELS[0].1)
        .click();
    harness.run();
    harness
        .get_by_label(
            "Linear elastic, permanent stretch due to creep given as a temperature increase",
        )
        .click();
    harness.run();
    harness.get_by_label("creep_temperature_shift");
    let text = {
        let r = harness.state_mut().ready().unwrap();
        r.libraries[&LibraryKind::Cables].text().unwrap()
    };
    assert!(
        text.contains("model = \"linear_creep_temperature\""),
        "{text}"
    );
    assert!(text.contains("creep_temperature_shift"));
    // Only the temperature shift is asked for.
    assert!(harness.query_by_label("creep_shift_tension").is_none());
    // Bimetallic options, the conductor's own: bird cage hides the
    // largest compressive stress, compression asks for it.
    harness
        .get_by_label(crate::library::BIMETALLIC_BEHAVIORS[1].1)
        .click();
    harness.run();
    assert!(harness.query_by_label("max_virtual_compression").is_none());
    {
        let r = harness.state_mut().ready().unwrap();
        let text = r.libraries[&LibraryKind::Cables].text().unwrap();
        assert!(
            text.contains("bimetallic_behavior = \"bird_cage\""),
            "{text}"
        );
    }
    harness
        .get_by_label(crate::library::BIMETALLIC_BEHAVIORS[2].1)
        .click();
    harness.run();
    harness.get_by_label("max_virtual_compression");
    {
        let r = harness.state_mut().ready().unwrap();
        let text = r.libraries[&LibraryKind::Cables].text().unwrap();
        assert!(
            text.contains("bimetallic_behavior = \"compression\""),
            "{text}"
        );
        assert!(text.contains("max_virtual_compression = \"68947.4 MPa\""));
    }
    // The line's choice: nothing written.
    harness
        .get_by_label(crate::library::BIMETALLIC_BEHAVIORS[0].1)
        .click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let text = r.libraries[&LibraryKind::Cables].text().unwrap();
        assert!(!text.contains("bimetallic_behavior"), "{text}");
    }
    // Unticking Bimetallic removes the core (and its options).
    harness.get_by_label(bimetallic).click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let page = &r.libraries[&LibraryKind::Cables];
    let text = page.text().unwrap();
    let first = text.split("[[cable]]").nth(1).unwrap();
    assert!(!first.contains("core"), "{first}");
    assert!(!first.contains("bimetallic_behavior"), "{first}");
    assert!(page.dirty());
}

/// Runs frames until `done` (at most 30 s: an exit clause).
pub(super) fn run_until(h: &mut Harness<'_, App>, mut done: impl FnMut(&mut App) -> bool) {
    let started = std::time::Instant::now();
    while !done(h.state_mut()) {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "not done in 30 s"
        );
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Plugins › Plugin manager lists the example plugins with what they
/// provide; turned off, a format plugin's formats go; File › Export and
/// Import into the line run on the worker (the import one command,
/// undoable).
#[test]
fn plugins_are_managed_and_give_their_formats() {
    let model = synthetic_project("plugins");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    let user = std::env::temp_dir().join(format!("ohl-app-plugins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&user);
    let examples = root().join("plugins/examples");
    let geojson_built = examples
        .join("geojson/target/wasm32-wasip2/release/geojson.wasm")
        .is_file();
    assert!(
        geojson_built || ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "geojson plugin not built"
    );
    {
        let app = harness.state_mut();
        app.plugins = crate::plugins::Plugins::load(None, Some(user.clone()));
        app.plugins.folders = vec![examples.clone()];
        app.plugins.refresh();
        app.ready().unwrap().plugin_request = Some(crate::app::PluginRequest::Manager);
    }
    run_until(&mut harness, |a| a.plugins.formats.is_some());
    harness.step();
    harness.get_by_label("Plugin manager");
    harness.get_by_label("hello-python");
    let has_geojson = |a: &App| {
        a.plugins
            .formats
            .as_ref()
            .unwrap()
            .exporters
            .iter()
            .any(|e| e.id == "geojson" && e.origin == "plugin geojson")
    };
    assert_eq!(has_geojson(harness.state_mut()), geojson_built);
    if geojson_built {
        harness.get_by_label_contains("export geojson");
        // Turned off: its formats go.
        let name = "geojson";
        harness.get_by_label(name).click();
        harness.step();
        run_until(&mut harness, |a| {
            a.plugins.formats.is_some() && !has_geojson(a)
        });
        assert!(harness.state_mut().plugins.disabled.contains(name));
    }
    // Install the Python example for the user.
    {
        let p = &mut harness.state_mut().plugins;
        p.install(&examples.join("hello-python"), false).unwrap();
        assert!(
            p.listed
                .iter()
                .any(|l| l.installed && l.found.name() == "hello-python")
        );
        assert!(
            p.listed.iter().any(|l| l.duplicate),
            "the folder's copy comes second"
        );
        // Not sandboxed: used once the user allowed it (security review
        // 2026-10-26).
        assert!(p.python_scripts().is_empty());
        let l = p
            .listed
            .iter()
            .find(|l| l.installed && l.found.name() == "hello-python")
            .unwrap();
        let asked = p.to_allow(l).unwrap();
        assert!(asked.unsandboxed);
        let key = l.found.manifest.as_ref().unwrap().consent_key();
        p.allowed.insert(key, asked);
        assert_eq!(p.python_scripts().len(), 1);
    }
    // Export the structures as CSV, move one in the file, import it back.
    let csv = user.join("structures.csv");
    let run = |h: &mut Harness<'_, App>, export: bool, format: &str| {
        let app = h.state_mut();
        let mut w = crate::plugins::FormatWindow {
            export,
            format: format.into(),
            ..Default::default()
        };
        let mut plugins = std::mem::take(&mut app.plugins);
        plugins.run(&mut w, &app.ready().unwrap().model, csv.clone());
        assert!(w.busy, "{:?}", w.error);
        plugins.window = Some(w);
        app.plugins = plugins;
    };
    run(&mut harness, true, "structures-csv");
    run_until(&mut harness, |a| a.plugins.window.is_none());
    let text = std::fs::read_to_string(&csv).unwrap();
    let first = text.lines().nth(1).unwrap().to_owned();
    let mut fields: Vec<String> = first.split(',').map(str::to_owned).collect();
    let id = fields[0].clone();
    let x: f64 = fields[1].parse().unwrap();
    fields[1] = format!("{}", x + 5.0);
    std::fs::write(&csv, text.replace(&first, &fields.join(","))).unwrap();
    run(&mut harness, false, "structures-csv");
    run_until(&mut harness, |a| a.plugins.window.is_none());
    harness.run();
    let position = |a: &mut App| {
        a.ready()
            .unwrap()
            .model
            .line
            .structures
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .position[0]
    };
    assert!((position(harness.state_mut()) - (x + 5.0)).abs() < 1e-6);
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    assert!((position(harness.state_mut()) - x).abs() < 1e-6, "undone");
    std::fs::remove_dir_all(&user).unwrap();
}

/// Plugins › Python console: a script reads the line and results, moves a
/// structure with a session command (solved again, undoable), undoes it;
/// a refused command raises in the script.
#[cfg(feature = "python")]
#[test]
fn the_python_console_drives_the_project() {
    use crate::python::Line;
    let _turn = crate::python::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let model = synthetic_project("python");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    crate::tests::submenu(&mut harness, "File", "Plugins");
    harness.run();
    harness.get_by_label("Python console").click();
    harness.run();
    let (id, x) = {
        let r = harness.state_mut().ready().unwrap();
        let s = &r.model.line.structures[1];
        (s.id.clone(), s.position)
    };
    let submit = |h: &mut Harness<'_, App>, code: String| {
        h.state_mut().ready().unwrap().console.submit(code);
        run_until(h, |a| a.ready().unwrap().console.running.is_none());
        h.state_mut().ready().unwrap().console.lines.last().cloned()
    };
    let out = submit(
        &mut harness,
        "len(ohl.line()['structures']), ohl.case(), len(ohl.results()['sections']) > 0".into(),
    );
    let n = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .structures
        .len();
    let case = harness.state_mut().ready().unwrap().model.cases[0]
        .case
        .name
        .clone();
    assert_eq!(out, Some(Line::Out(format!("({n}, '{case}', True)\n"))));
    // Move a structure 3 m east.
    submit(
        &mut harness,
        format!(
            "p = [s for s in ohl.line()['structures'] if s['id'] == '{id}'][0]['position']\n\
             p[0] += 3.0\n\
             ohl.apply({{'op': 'move_structure', 'id': '{id}', 'position': p}}, 'moved by a script')"
        ),
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.line.structures.iter().find(|s| s.id == id).unwrap();
        assert!((s.position[0] - x[0] - 3.0).abs() < 1e-9);
        assert_solved_fully(&r.model);
    }
    submit(&mut harness, "ohl.undo()".into());
    {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.line.structures.iter().find(|s| s.id == id).unwrap();
        assert_eq!(s.position, x, "undone");
    }
    // Refused: raised in the script.
    let out = submit(
        &mut harness,
        "ohl.apply({'op': 'move_structure', 'id': 'no-such', 'position': [0, 0, 0]})".into(),
    );
    assert!(
        matches!(&out, Some(Line::Err(t)) if t.contains("no-such")),
        "{out:?}"
    );
    harness.get_by_label_contains("Python 3.");
}

/// Server mode: two applications join a hosted project (File › Join a
/// server); an edit in one reaches the other; a structure in a Move
/// window is held; an admin makes a viewer a designer; undo is per user.
#[test]
fn two_applications_share_a_hosted_project() {
    use crate::app::MoveDialog;
    let model = synthetic_project("shared");
    let file = model.sources.file.clone();
    drop(model);
    {
        let s = ohl_session::Session::open(&file).unwrap();
        s.set_user("tester", "bob", ohl_session::Role::Viewer)
            .unwrap();
    }
    let server = ohl_server::server::Server::start(
        &file,
        ohl_server::server::Options {
            bind: "127.0.0.1:0".parse().unwrap(),
            advertise: None,
            code: None,
            keys: None,
            newcomers: None,
            web: None,
            ..Default::default()
        },
    )
    .unwrap();
    let dir = std::env::temp_dir().join(format!("ohl-app-shared-{}", std::process::id()));
    let open = |user: &str| {
        let mut app = App::empty(Sources::default(), None);
        app.joined_dir = dir.join(user);
        app.join_window = Some(crate::remote::JoinWindow {
            invitation: server.invitation_for(user).to_string(),
            user: user.into(),
            ..Default::default()
        });
        let mut h = Harness::builder()
            .with_size(egui::vec2(1500.0, 950.0))
            .build_eframe(|_| app);
        h.step();
        h.get_by_label("Join").click();
        run_until(&mut h, |a| {
            a.ready()
                .is_some_and(|r| r.model.remote.is_some() && !r.model.cases.is_empty())
        });
        h
    };
    let mut alice = open("tester");
    let mut bob = open("bob");
    assert!(
        bob.state_mut().ready().unwrap().model.can_edit().is_err(),
        "a viewer"
    );
    let id = alice.state_mut().ready().unwrap().model.line.structures[1]
        .id
        .clone();
    let p0 = alice
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .structure(&id)
        .unwrap()
        .position;
    let at = |h: &mut Harness<'_, App>| {
        h.state_mut()
            .ready()
            .unwrap()
            .model
            .line
            .structure(&id)
            .unwrap()
            .position
    };

    // Alice moves it: Bob's line follows, re-solved.
    let p1 = [p0[0] + 4.0, p0[1], p0[2]];
    assert!(
        alice
            .state_mut()
            .ready()
            .unwrap()
            .perform(crate::edit::Action::Move {
                id: id.clone(),
                position: p1,
            })
    );
    assert_eq!(at(&mut alice), p1);
    run_until(&mut bob, |a| {
        a.ready()
            .unwrap()
            .model
            .line
            .structure(&id)
            .unwrap()
            .position
            == p1
    });
    assert_solved_fully(&bob.state_mut().ready().unwrap().model);
    // Both see each other.
    let peers = |h: &mut Harness<'_, App>| {
        h.state_mut()
            .ready()
            .unwrap()
            .model
            .remote
            .as_ref()
            .unwrap()
            .peers
            .len()
    };
    run_until(&mut alice, |a| {
        a.ready()
            .unwrap()
            .model
            .remote
            .as_ref()
            .unwrap()
            .peers
            .len()
            == 2
    });
    assert_eq!(peers(&mut bob), 2);

    // The admin makes Bob a designer: he can edit.
    alice.state_mut().ready().unwrap().project_op(
        "bob made designer",
        ohl_server::Op::SetUser {
            name: "bob".into(),
            role: ohl_session::Role::Designer,
        },
    );
    run_until(&mut bob, |a| a.ready().unwrap().model.can_edit().is_ok());

    // Alice's Move window holds the structure: Bob cannot take it.
    alice.state_mut().ready().unwrap().move_dialog = Some(MoveDialog {
        id: id.clone(),
        start: p1,
        position: p1,
        on_ground: false,
        shown: p1,
    });
    alice.step();
    let target = ohl_server::lock_target("main", ohl_session::Kind::Structure, &id);
    run_until(&mut bob, |a| {
        a.ready().unwrap().model.locked_by_other(&target).is_some()
    });
    {
        let r = bob.state_mut().ready().unwrap();
        r.hotbar.hold(crate::tools::Tool::Move);
        r.use_tool_on(&id);
        assert!(r.move_dialog.is_none());
        let status = r.status.clone().unwrap().0;
        assert!(status.contains("being moved by tester"), "{status}");
        // The server refuses his command too.
        assert!(!r.perform(crate::edit::Action::Move {
            id: id.clone(),
            position: p0,
        }));
    }
    alice.state_mut().ready().unwrap().move_dialog = None;
    alice.step();
    run_until(&mut bob, |a| {
        a.ready().unwrap().model.locked_by_other(&target).is_none()
    });

    // Bob's undo: nothing of his; Alice undoes her move.
    {
        let r = bob.state_mut().ready().unwrap();
        assert!(!r.perform(crate::edit::Action::Undo));
    }
    assert!(
        alice
            .state_mut()
            .ready()
            .unwrap()
            .perform(crate::edit::Action::Undo)
    );
    run_until(&mut bob, |a| {
        a.ready()
            .unwrap()
            .model
            .line
            .structure(&id)
            .unwrap()
            .position
            == p0
    });
    // People lists both, with the invitation.
    {
        let r = alice.state_mut().ready().unwrap();
        r.open_page = Some((crate::app::Tab::People, None));
    }
    alice.step();
    alice.step();
    alice.get_by_label("Copy");
    alice.get_by_label_contains("(you)");
    // Roles: what each may do, her own role marked (owner, 2026-10-26).
    {
        let r = alice.state_mut().ready().unwrap();
        r.open_page = Some((crate::app::Tab::Roles, None));
    }
    alice.step();
    alice.step();
    alice.get_by_label("Admin (you)");
    alice.get_by_label_contains("Pass other users' locks");
    drop(alice);
    drop(bob);
    server.stop();
    let s = ohl_session::Session::open(&file).unwrap();
    assert_eq!(s.line().structure(&id).unwrap().position, p0);
    assert_eq!(s.role("bob").unwrap(), Some(ohl_session::Role::Designer));
    let _ = std::fs::remove_dir_all(&dir);
}

/// File › Host this project: the application hosts and joins its own
/// server (its edits go through it); Stop hosting opens the project file
/// again, with the edits.
#[test]
fn an_application_hosts_its_project() {
    let model = synthetic_project("hosting");
    let file = model.sources.file.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    {
        let app = harness.state_mut();
        app.joined_dir = file.parent().unwrap().join("joined");
        app.template.user = Some("tester".into());
        app.file_action = Some(crate::files::FileAction::Host);
    }
    harness.step();
    {
        let w = harness.state_mut().host_window.as_mut().unwrap();
        w.port = 0;
        w.bind = std::net::IpAddr::from([127, 0, 0, 1]);
    }
    harness.step();
    harness.get_by_label("Start hosting").click();
    run_until(&mut harness, |a| {
        // A failure to host or join says why.
        if let Some(e) = a.host_window.as_ref().and_then(|w| w.error.clone()) {
            panic!("hosting: {e}");
        }
        a.hosting.is_some()
            && a.ready()
                .is_some_and(|r| r.model.remote.is_some() && !r.model.cases.is_empty())
    });
    let (id, p0) = {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.model.can_edit().is_ok());
        assert!(r.model.remote.as_ref().unwrap().hosting);
        let s = &r.model.line.structures[0];
        (s.id.clone(), s.position)
    };
    let p1 = [p0[0], p0[1] + 3.0, p0[2]];
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .perform(crate::edit::Action::Move {
                id: id.clone(),
                position: p1,
            })
    );
    // Stop hosting: the project file itself again, moved.
    harness.state_mut().file_action = Some(crate::files::FileAction::Leave);
    run_until(&mut harness, |a| {
        a.hosting.is_none()
            && a.ready()
                .is_some_and(|r| r.model.remote.is_none() && r.model.sources.file == file)
    });
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.line.structure(&id).unwrap().position, p1);
    assert!(r.model.can_edit().is_ok());
}

/// Checks › Structure loads: computed in the background from the
/// project's load cases (kept in the project), shown per load case; a
/// structure's link goes to it.
#[test]
fn structure_loads_are_computed_and_shown() {
    let model = synthetic_project("loads");
    assert!(
        model
            .sources
            .load_cases
            .as_ref()
            .is_some_and(|p| p.ancestors().any(|a| a.ends_with("ohl-design-libraries"))),
        "kept in the project: {:?}",
        model.sources.load_cases
    );
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label("Compute structure loads").click();
    run_until(&mut harness, |a| {
        let r = a.ready().unwrap();
        r.loads.poll();
        r.loads.results.is_some()
    });
    harness.step();
    harness.step();
    {
        let r = harness.state_mut().ready().unwrap();
        let results = r.loads.results.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(results.len(), 3);
        assert!(results[0].failed.is_empty());
        assert!(!results[0].loads.is_empty());
    }
    harness.get_by_label("transverse kN");
    harness.get_all_by_label("T2").next().unwrap().click();
    harness.step();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.selection, Selection::Structure("T2".into()));
        // The usage came with the loads: this line has no structure
        // types, so every structure is listed, not checked.
        assert_eq!(r.loads.usage.len(), 3);
        assert!(r.loads.usage[0].structures.iter().all(|s| {
            s.governing().is_none() && s.notes.iter().any(|n| n.contains("no structure type"))
        }));
    }
    harness.get_by_label("Usage").click();
    harness.step();
    harness.step();
    harness.get_by_label("weight span m");
    assert!(
        harness
            .query_all_by_label_contains("0 structures checked")
            .next()
            .is_some()
    );
}

/// Only the weather case shown is solved when the model loads; another is
/// solved in the background once shown, from the line as it is then;
/// cases never shown stay unsolved.
#[test]
fn weather_cases_are_solved_when_shown() {
    let ex = root().join("examples/synthetic-line");
    let model = model::load(
        Sources {
            file: ex.join("synthetic-line.ohl.json"),
            library: Some(root().join("libraries/cables")),
            weather: Some(ex.join("weather-cases.ohlwc")),
            point_budget: 1000,
            corridor: 60.0,
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    assert!(model.cases[0].ready);
    assert!(model.cases.len() > 2);
    assert!(
        model.cases[1..]
            .iter()
            .all(|c| !c.ready && c.shapes.is_empty())
    );
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.step();
    // Show the third case (Display weather cases): solved in the
    // background.
    harness.state_mut().ready().unwrap().model.shown_cases = vec![2];
    run_until(&mut harness, |a| a.ready().unwrap().model.cases[2].ready);
    let m = &harness.state_mut().ready().unwrap().model;
    assert!(!m.cases[1].ready, "not shown: not solved");
    assert_solved_fully(m);
    assert!(!m.cases[2].shapes.is_empty());
}

/// Terrain › Terrain surfaces (TIN): a survey file imported becomes the
/// ground (none before); another surface (LandXML) is shown in its colour;
/// those choices are the user's (remembered locally, not in the project):
/// opened again with them, the surface chosen is the ground; rename and
/// delete.
#[test]
fn terrain_surfaces_are_kept_in_the_project() {
    use crate::surfaces::Action;
    let model = synthetic_project("surfaces");
    let file = model.sources.file.clone();
    assert!(model.ground.is_none());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.step();
    let act = |h: &mut Harness<'_, App>, a: Action| {
        let ctx = egui::Context::default();
        let app = h.state_mut();
        let mut plugins = std::mem::take(&mut app.plugins);
        let r = app.ready().unwrap();
        let project = r.model.sources.file.clone();
        r.surfaces
            .act(a, &mut r.model, &mut plugins, "tester", &project, &ctx);
        app.plugins = plugins;
    };
    {
        let r = harness.state_mut().ready().unwrap();
        r.surfaces.show(&r.model);
    }
    // Import the survey points: triangulated, kept, made the ground.
    // (Nothing waiting to be drawn: the import must ask for the ground.)
    harness.state_mut().ready().unwrap().view3d.dirty.clear();
    let survey = root().join("examples/synthetic-line/synthetic-survey.csv");
    act(&mut harness, Action::Import(survey));
    run_until(&mut harness, |a| {
        a.ready().unwrap().model.ground_source == "surface synthetic-survey"
    });
    harness.get_by_label("Terrain surfaces (TIN)");
    {
        // Drawn at once (no reopening), shown.
        let r = harness.state_mut().ready().unwrap();
        assert!(r.view3d.dirty.contains(&crate::scene::Layer::Ground));
        assert!(r.visible.contains(&crate::scene::Layer::Ground));
    }
    let n = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .ground
        .as_ref()
        .unwrap()
        .num_triangles();
    assert!(n > 100);
    // A second surface (LandXML of the first, raised 1 m), shown.
    let dir = file.parent().unwrap().to_owned();
    let xml = dir.join("raised.xml");
    {
        let g = harness
            .state_mut()
            .ready()
            .unwrap()
            .model
            .ground
            .clone()
            .unwrap();
        let raised = ohl_terrain::Terrain::from_mesh(
            g.points()
                .iter()
                .map(|p| [p[0], p[1], p[2] + 1.0])
                .collect(),
            g.triangles().to_vec(),
        )
        .unwrap();
        std::fs::write(
            &xml,
            ohl_terrain::formats::write_landxml(&[("raised", &raised)]),
        )
        .unwrap();
    }
    act(&mut harness, Action::Import(xml));
    run_until(&mut harness, |a| {
        a.ready().unwrap().surfaces.list.len() == 2
    });
    act(&mut harness, Action::Show("raised".into(), true));
    run_until(&mut harness, |a| {
        a.ready().unwrap().model.other_surfaces.len() == 1
    });
    // The choice is this user's, not the project's.
    let prefs = harness.state_mut().ready().unwrap().model.view_prefs();
    {
        let s = ohl_session::Session::open(&file).unwrap();
        assert_eq!(s.setting(crate::surfaces::SETTING).unwrap(), None);
    }
    // Opened again with this user's choice: the surface chosen is the
    // ground (no LiDAR, no survey).
    let again = model::load(
        Sources {
            file: file.clone(),
            library: Some(root().join("libraries/cables")),
            point_budget: 1000,
            corridor: 60.0,
            user: Some("tester".into()),
            view: Some(prefs),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    assert_eq!(again.ground_source, "surface synthetic-survey");
    assert_eq!(again.ground.as_ref().unwrap().num_triangles(), n);
    assert_eq!(again.other_surfaces.len(), 1);
    assert_eq!(again.other_surfaces[0].name, "raised");
    drop(again);
    // The checks use "raised" for everyone (a project setting), while
    // this user's ground stays the survey.
    act(&mut harness, Action::Check(Some("raised".into())));
    run_until(&mut harness, |a| {
        a.ready().unwrap().model.check_ground.is_some()
    });
    {
        let r = harness.state_mut().ready().unwrap();
        let (check, shown) = (
            r.model.ground_for_checks().unwrap(),
            r.model.ground.clone().unwrap(),
        );
        let (x, y) = (shown.points()[0][0], shown.points()[0][1]);
        assert!((check.height(x, y).unwrap() - shown.height(x, y).unwrap() - 1.0).abs() < 1e-9);
        let s = ohl_session::Session::open(&file).unwrap();
        assert_eq!(
            s.setting(crate::surfaces::CHECK_SETTING)
                .unwrap()
                .as_deref(),
            Some("raised")
        );
    }
    // Rename the ground; delete the other.
    act(
        &mut harness,
        Action::Rename("synthetic-survey".into(), "survey".into()),
    );
    act(&mut harness, Action::Delete("raised".into()));
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.ground_source, "surface survey");
    assert!(r.model.other_surfaces.is_empty());
    let names: Vec<&str> = r.surfaces.list.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["survey"]);
    assert_eq!(r.model.surface_settings.active.as_deref(), Some("survey"));
}

/// File › New project from another program (no file open): a project
/// plugin's format read on the worker into a new project, opened, its
/// notes shown.
#[test]
fn a_project_is_created_from_another_programs_file() {
    let dir = root().join("plugins/examples/project-bundle");
    let built = dir
        .join("target/wasm32-wasip2/release/project_bundle.wasm")
        .exists();
    assert!(
        built || ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "project-bundle plugin not built"
    );
    if !built {
        return;
    }
    let tmp = std::env::temp_dir().join(format!("ohl-app-import-project-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let ex = root().join("examples/synthetic-line");
    let bundle = serde_json::json!({
        "line": serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(ex.join("synthetic-line.ohl.json")).unwrap()).unwrap(),
        "libraries": [
            {"kind": "cables", "name": "acsr-examples.ohlw",
             "text": std::fs::read_to_string(root().join("libraries/cables/acsr-examples.ohlw")).unwrap()},
            {"kind": "weather", "name": "weather-cases.ohlwc",
             "text": std::fs::read_to_string(ex.join("weather-cases.ohlwc")).unwrap()},
        ],
    });
    let file = tmp.join("line.ohlb");
    std::fs::write(&file, bundle.to_string()).unwrap();
    let mut app = App::empty(Sources::default(), None);
    app.plugins = crate::plugins::Plugins::load(None, None);
    app.plugins.folders = vec![dir];
    app.plugins.refresh();
    app.file_action = Some(crate::files::FileAction::ImportProject);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| app);
    h.step();
    h.step();
    h.get_by_label("New project from another program");
    let into = tmp.join("imported.ohl");
    h.state_mut()
        .plugins
        .create_project(crate::plugins::ProjectImport {
            path: file,
            format: None,
            options: [("name".to_owned(), "Imported".to_owned())].into(),
            into: into.clone(),
            user: crate::files::user(),
            clip_in: true,
        });
    run_until(&mut h, |a| {
        a.ready()
            .is_some_and(|r| r.model.line.name == "Imported" && !r.model.cases.is_empty())
    });
    h.step();
    h.get_by_label("Project created");
    let app = h.state_mut();
    let notes = &app.import_notes.as_ref().unwrap().1;
    assert!(notes.iter().any(|n| n.contains("clipped in")), "{notes:?}");
    let r = app.ready().unwrap();
    assert_eq!(r.model.sources.file, into);
    assert!(r.model.line.sections.iter().all(|s| s.stringing.clipped()));
    // Its libraries are the project's: five weather cases.
    assert_eq!(r.model.cases.len(), 5);
    let _ = std::fs::remove_dir_all(&tmp);
}
