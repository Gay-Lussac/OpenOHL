// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: the String builder (owner 2026-10-31, "the builder window with a
//! torus").

use super::*;

/// The typed example with type-02's strings (T2's) of the example's
/// INVENTED assembly with volumes (its length and weight those of the
/// strings), its string library a copy in the project's folder (never the
/// example's files).
fn project_with_assemblies(name: &str) -> model::Model {
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    for folder in ["Structures", "Insulators"] {
        std::fs::create_dir_all(dir.join(folder)).unwrap();
        for e in std::fs::read_dir(ex.join(folder)).unwrap() {
            let p = e.unwrap().path();
            std::fs::copy(&p, dir.join(folder).join(p.file_name().unwrap())).unwrap();
        }
    }
    let path = dir.join("Structures/type-02.ohls");
    let text = std::fs::read_to_string(&path).unwrap();
    let given = "length = \"3.500 m\"\nweight = \"900.0 N\"";
    assert_eq!(text.matches(given).count(), 2);
    std::fs::write(
        &path,
        text.replace(given, "insulator = \"invented-i-volumes\""),
    )
    .unwrap();
    let file = dir.join("p.ohl");
    let line = ohl_line::Line::load(&ex.join("synthetic-line-typed.ohl.json")).unwrap();
    ohl_session::Session::open(&file)
        .unwrap()
        .import(&line, "tester")
        .unwrap();
    solved_all(
        model::load(
            Sources {
                file,
                library: Some(root().join("libraries/cables")),
                weather: Some(ex.join("weather-cases.ohlwc")),
                insulators: Some(dir.join("Insulators")),
                point_budget: 1000,
                corridor: 60.0,
                user: Some("tester".into()),
                ..Sources::default()
            },
            &|_| {},
        )
        .unwrap(),
    )
}

/// Line › Structures › String builder… opens the window on the selected
/// string's assembly ([`project_with_assemblies`]); a preset and a
/// grading ring (a torus, INVENTED sizes) added with their buttons, the
/// preview drawn; Save writes the assembly's file (version 5, the torus in
/// it) as one undoable change; the internal clearance check measures the
/// ring (an INVENTED 50 m rule: every part too near) against T2's body.
#[test]
fn the_string_builder_builds_a_grading_ring_the_check_measures() {
    let model = project_with_assemblies("string-builder");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    let (label, path) = {
        let r = h.state_mut().ready().unwrap();
        let t2 = r.model.line.structure("T2").unwrap();
        let x = &t2.strings[0];
        assert_eq!(x.insulator.as_deref(), Some("invented-i-volumes"));
        let label = x.label.clone();
        let path = r
            .model
            .types
            .as_ref()
            .unwrap()
            .1
            .path("invented-i-volumes")
            .unwrap()
            .to_path_buf();
        (label, path)
    };
    // Never the example's files: the project's copy.
    assert!(!path.starts_with(root()), "{}", path.display());
    h.state_mut().ready().unwrap().select(Selection::String {
        structure: "T2".into(),
        label,
    });
    line_menu(&mut h, "Structures");
    h.get_by_label("String builder…").click();
    h.run();
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        assert!(r.string_builder.open);
        assert_eq!(r.string_builder.assembly, "invented-i-volumes");
        assert!(r.string_builder.draft.is_some());
    }
    h.get_by_label("I string").click();
    h.run();
    h.get_by_label("+ Grading ring").click();
    h.run();
    // The preview's two views drawn.
    h.get_by_label("String builder preview, front view");
    h.get_by_label("String builder preview, side view");
    {
        let r = h.state_mut().ready().unwrap();
        let d = r.string_builder.draft.as_ref().unwrap();
        assert!(d.changed());
        let ring = d.volumes.shapes.last().unwrap();
        assert_eq!(ring.kind, ohl_structures::ShapeKind::Torus);
        assert_eq!(r.string_builder.selected, Some(d.volumes.shapes.len() - 1));
    }
    // Its fields shown: the tube's among them.
    h.get_by_label("Tube");
    // An INVENTED ring 2 m across: wide, to come near the body.
    if let Some(d) = h.state_mut().ready().unwrap().string_builder.draft.as_mut() {
        let ring = d.volumes.shapes.last_mut().unwrap();
        ring.diameter = 2.0;
        ring.tube = 0.1;
    }
    h.run();
    // The window as drawn (OHL_RENDER_PNG set: a picture for the docs).
    if let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
        && let Ok(image) = h.render()
    {
        let path = PathBuf::from(dir).join("string-builder.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    h.get_by_label("Save to the library").click();
    h.run();
    h.run();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("version = 5") && text.contains("kind = \"torus\""),
        "{text}"
    );
    {
        let r = h.state_mut().ready().unwrap();
        let a = r
            .model
            .types
            .as_ref()
            .unwrap()
            .1
            .get("invented-i-volumes")
            .unwrap();
        assert!(a.volumes.has_torus());
        assert!(!r.string_builder.draft.as_ref().unwrap().changed());
    }
    // One undo: the library's assembly without the ring; the builder,
    // without changes, follows.
    h.state_mut()
        .ready()
        .unwrap()
        .perform(crate::edit::Action::Undo);
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        let a = r
            .model
            .types
            .as_ref()
            .unwrap()
            .1
            .get("invented-i-volumes")
            .unwrap();
        assert!(!a.volumes.has_torus());
        let d = r.string_builder.draft.as_ref().unwrap();
        assert!(!d.volumes.has_torus() && !d.changed());
    }
    h.state_mut()
        .ready()
        .unwrap()
        .perform(crate::edit::Action::Redo);
    h.run();
    // The internal clearance check: the ring on T2's strings measured.
    let ctx = h.ctx.clone();
    {
        let r = h.state_mut().ready().unwrap();
        assert!(
            r.model
                .types
                .as_ref()
                .unwrap()
                .1
                .get("invented-i-volumes")
                .unwrap()
                .volumes
                .has_torus()
        );
        r.internal_clearances.show(&r.model);
        let mut rule = ohl_clearance::internal::InternalRule::blank("invented 50 m");
        rule.distance = ohl_clearance::Requirement::Fixed(50.0);
        r.internal_clearances.rules = vec![rule];
        r.internal_clearances.start(&r.model, &ctx);
    }
    let started = std::time::Instant::now();
    loop {
        h.step();
        let r = h.state_mut().ready().unwrap();
        if let Some(res) = &r.internal_clearances.job.result {
            let rep = res.as_ref().unwrap();
            let ring = rep
                .report
                .rows
                .iter()
                .find(|x| x.structure == "T2" && x.part.to_string().ends_with("grading ring"))
                .unwrap_or_else(|| {
                    panic!(
                        "no ring row: {:?}",
                        rep.report
                            .rows
                            .iter()
                            .map(|x| x.part.to_string())
                            .collect::<Vec<_>>()
                    )
                });
            assert!(ring.distance < ring.required, "{ring:?}");
            break;
        }
        assert!(started.elapsed().as_secs() < 120, "no report in 120 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}
