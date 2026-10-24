// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: backgrounds, the ground built, project lines and the working
//! tree, imports.

use super::*;

/// A synthetic project with an orthophoto (layer "Ortho") and a DXF
/// drawing ("cadastre") over the line, read into its model.
pub(super) fn project_with_backgrounds(name: &str) -> model::Model {
    let mut model = synthetic_project(name);
    let file = model.sources.file.clone();
    let dir = file.parent().unwrap().to_path_buf();
    // 1400 m × 700 m at 2 m per pixel over the line (x 0…1250, y 0…560).
    let png = dir.join("ortho.png");
    image::RgbImage::from_fn(700, 350, |x, y| {
        image::Rgb([(x % 200) as u8 + 30, (y % 200) as u8 + 30, 90])
    })
    .save(&png)
    .unwrap();
    std::fs::write(dir.join("ortho.pgw"), "2\n0\n0\n-2\n-49\n609\n").unwrap();
    let report =
        crate::backgrounds::import_images(&file, "Ortho", &[png], "", "", &|_, _| {}).unwrap();
    assert!(report.starts_with("1 image(s)"), "{report}");
    let dxf = dir.join("cadastre.dxf");
    std::fs::write(
        &dxf,
        "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\nParcels\n62\n3\n70\n1\n38\n500\n\
         10\n100\n20\n-40\n10\n600\n20\n-40\n10\n600\n20\n40\n10\n100\n20\n40\n\
         0\nTEXT\n8\nParcels\n10\n300\n20\n0\n40\n8\n1\nParcel 7\n0\nENDSEC\n0\nEOF\n",
    )
    .unwrap();
    ohl_background::Store::open(&file)
        .unwrap()
        .import_dxf("cadastre", &dxf, "")
        .unwrap();
    let mut log = Vec::new();
    model.backgrounds = crate::backgrounds::load(&file, &mut log);
    model
}

#[test]
fn backgrounds_are_kept_shown_and_hidden() {
    use crate::scene::Layer;
    let model = project_with_backgrounds("backgrounds");
    assert_eq!(
        model.backgrounds.image_layers(),
        vec![("Ortho".to_owned(), 1)]
    );
    assert_eq!(model.backgrounds.drawings.len(), 1);
    // The drawing in 3D at its heights; orthophotos in plan only.
    let s = crate::scene::layer(&model, Layer::Drawings, 0, &Selection::None);
    assert!(s.counts().0 >= 4, "{:?}", s.counts());
    let s = crate::scene::layer(&model, Layer::Orthophotos, 0, &Selection::None);
    assert_eq!(s.counts(), (0, 0, 0));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // Shown by default.
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.visible.contains(&Layer::Orthophotos) && r.visible.contains(&Layer::Drawings));
    }
    // Terrain › Backgrounds: two boxes ticked in a row (the menu stays
    // open).
    harness.get_by_label("Terrain").click();
    harness.run();
    harness.get_by_label_contains("Backgrounds").hover();
    harness.run();
    harness.get_by_label_contains("Ortho (1)").click();
    harness.run();
    harness.get_by_label_contains("cadastre").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.hidden_backgrounds.contains("image:Ortho"));
    assert!(r.model.hidden_backgrounds.contains("drawing:cadastre"));
    // Remembered with this user's view.
    assert_eq!(r.model.view_prefs().hidden_backgrounds.len(), 2);
    let s = crate::scene::layer(&r.model, Layer::Drawings, 0, &Selection::None);
    assert_eq!(s.counts().0, 0, "hidden in 3D too");
    // A drawing hidden: its chunks made again; an edit of the line or
    // the whole layer hidden: kept (a large drawing takes a while).
    assert!(r.view3d.dirty.contains(&Layer::Drawings));
    r.view3d.dirty.clear();
    r.refresh_views();
    assert!(r.view3d.dirty.contains(&Layer::Structures));
    assert!(!r.view3d.dirty.contains(&Layer::Drawings));
    r.view3d.dirty.clear();
    harness
        .get_by_label_contains("Drawings (plan and 3D views)")
        .click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(!r.visible.contains(&Layer::Drawings));
    assert!(!r.view3d.dirty.contains(&Layer::Drawings));
}

#[test]
#[ignore = "writes images of the backgrounds to OHL_SHOTS (a look by eye)"]
fn background_images() {
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let model = project_with_backgrounds("bg-shots");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().show_tab(crate::app::Tab::Plan);
    for _ in 0..10 {
        harness.run();
    }
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/plan.png"))
        .unwrap();
    harness.state_mut().show_tab(crate::app::Tab::View3d);
    for _ in 0..5 {
        harness.run();
    }
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/3d.png"))
        .unwrap();
}

/// Terrain surfaces › Build from LiDAR: the model's classes in a list to
/// tick (the ground classes at first); nothing to tick without LiDAR.
#[test]
fn the_ground_is_built_from_the_classes_ticked() {
    use egui_kittest::kittest::NodeT;
    let mut model = synthetic_project("build-classes");
    model.points = vec![
        ([0, 0, 0], 2),
        ([1000, 0, 0], 3),
        ([2000, 0, 0], 5),
        ([3000, 0, 0], 6),
    ];
    model.class_counts = vec![(2, 40), (3, 10), (5, 7), (6, 2)];
    model.cloud = Some("survey.ohl".into());
    let classes = crate::surfaces::lidar_classes(&model);
    assert_eq!(
        classes.iter().map(|c| (c.0, c.2)).collect::<Vec<_>>(),
        [(2, 40), (3, 10), (5, 7), (6, 2)]
    );
    let label = |code: u8| {
        let c = classes.iter().find(|c| c.0 == code).unwrap();
        format!("{} {} ({} points)", c.0, c.1, c.2)
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(
            r.surfaces.lidar.chosen(&classes),
            [2].into(),
            "the ground classes"
        );
        r.surfaces.show(&r.model);
    }
    harness.run();
    harness.get_by_label(&label(3)).click();
    harness.run();
    harness.get_by_label(&label(2)).click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.surfaces.lidar.classes, Some([3].into()));
    // Without LiDAR: the list is empty and Build cannot be pressed.
    let model = synthetic_project("build-no-lidar");
    assert!(crate::surfaces::lidar_classes(&model).is_empty());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.surfaces.show(&r.model);
    }
    harness.run();
    harness.get_by_label("The model has no LiDAR points.");
    assert!(harness.get_by_label("Build").accesskit_node().is_disabled());
}

#[test]
#[ignore = "writes an image of the surfaces window to OHL_SHOTS (a look by eye)"]
fn surfaces_window_image() {
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let mut model = synthetic_project("surfaces-shot");
    model.points = vec![
        ([0, 0, 0], 2),
        ([1000, 0, 0], 3),
        ([2000, 0, 0], 5),
        ([3000, 0, 0], 6),
    ];
    model.class_counts = vec![
        (1, 900),
        (2, 40),
        (3, 10),
        (4, 12),
        (5, 7),
        (6, 2),
        (7, 1),
        (9, 3),
        (17, 4),
    ];
    model.cloud = Some("survey.ohl".into());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.surfaces.show(&r.model);
    }
    harness.run();
    harness.run();
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/surfaces.png"))
        .unwrap();
}

/// A long list of errors scrolls in a box: the window stays on the screen
/// and can be closed (the rule for every message in a window).
#[test]
fn long_errors_scroll_and_the_window_closes() {
    let model = synthetic_project("long-errors");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let long: String = (0..200)
        .map(|k| format!("tile_{k:03}.ecw: an ECW file needs a converter\n"))
        .collect();
    {
        let ctx = harness.ctx.clone();
        let r = harness.state_mut().ready().unwrap();
        r.backgrounds.job = Some(crate::survey_io::Job::start(
            "Import".into(),
            true,
            &ctx,
            move |_| Err(long),
        ));
    }
    run_until(&mut harness, |a| {
        a.ready()
            .unwrap()
            .backgrounds
            .job
            .as_mut()
            .is_some_and(|j| {
                j.poll();
                j.done.is_some()
            })
    });
    harness.run();
    let close = harness.get_by_label("Close").rect();
    assert!(close.max.y < 900.0, "Close at {close:?}, off the screen");
    harness.get_by_label("Close").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.backgrounds.job.is_none());
    assert!(!r.reopen, "a failed import opens nothing again");
}

#[test]
fn image_import_failures_are_grouped_by_reason() {
    let model = synthetic_project("grouped");
    let dir = model.sources.file.parent().unwrap().to_path_buf();
    let files: Vec<PathBuf> = (0..8).map(|k| dir.join(format!("t{k}.ecw"))).collect();
    let e = crate::backgrounds::import_images(&model.sources.file, "O", &files, "", "", &|_, _| {})
        .unwrap_err();
    assert_eq!(e.lines().count(), 2, "{e}");
    assert!(
        e.contains("8 file(s): an ECW file needs a converter"),
        "{e}"
    );
    assert!(e.contains("and 3 more"), "{e}");
}

/// Drawings in the level of detail: chunked like the other 3D layers,
/// simplified at levels 1 and 2 to the row's tolerance; a row of their
/// own in the Level of detail table.
#[test]
fn drawings_have_levels_of_detail() {
    use crate::lod::{CHUNKED, DetailKind, LodSettings};
    use crate::scene::Layer;
    assert!(CHUNKED.contains(&Layer::Drawings));
    assert_eq!(DetailKind::of(Layer::Drawings), DetailKind::Tolerance);
    let row = LodSettings::default().row(Layer::Drawings);
    assert_eq!(
        (row.render_distance, row.from, row.detail),
        (10_000.0, [300.0, 2_000.0], [0.2, 1.5])
    );
    row.check(Layer::Drawings).unwrap();
    // Douglas–Peucker: a fine arc of radius 100 m (1° steps) within 0.5 m.
    let arc: Vec<[f64; 3]> = (0..=90)
        .map(|k| {
            let a = f64::from(k).to_radians();
            [100.0 * a.cos(), 100.0 * a.sin(), 0.0]
        })
        .collect();
    let s = crate::chunks::simplify(&arc, 0.5);
    assert!(s.len() < 20 && s.len() >= 3, "{}", s.len());
    assert_eq!((s[0], *s.last().unwrap()), (arc[0], arc[90]));
    // The test drawing: a 500 m parcel crossing tiles, cut into pieces.
    let mut model = project_with_backgrounds("lod-drawings");
    let index = crate::chunks::ChunkIndex::new(&model, Layer::Drawings, 0);
    assert!(index.len() >= 2, "{}", index.len());
    let lines = |level: usize, model: &crate::model::Model| -> usize {
        let index = crate::chunks::ChunkIndex::new(model, Layer::Drawings, 0);
        index
            .keys()
            .map(|(t, _)| {
                index
                    .build(model, Layer::Drawings, &t, level, &row)
                    .counts()
                    .0
            })
            .sum()
    };
    let full = lines(0, &model);
    assert!(full >= 4, "{full}");
    // A 1 m line: drawn at level 0, left out at level 2 (1.5 m tolerance).
    let d = std::sync::Arc::make_mut(&mut model.backgrounds.drawings[0].drawing);
    d.lines.push(ohl_background::dxf::Polyline {
        layer: 0,
        color: [255, 0, 0],
        points: vec![[300.0, 300.0, 500.0], [301.0, 300.0, 500.0]],
        closed: false,
    });
    assert_eq!(lines(0, &model), full + 1);
    assert_eq!(lines(2, &model), full);
    // Hidden: no chunks.
    model.hidden_backgrounds.insert("drawing:cadastre".into());
    assert_eq!(
        crate::chunks::ChunkIndex::new(&model, Layer::Drawings, 0).len(),
        0
    );
}

#[test]
#[ignore = "writes images of the Lines menu and windows to OHL_SHOTS (a look by eye)"]
fn lines_images() {
    use crate::project_lines::LinesWindow;
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let model = synthetic_project("lines-shots");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        for n in ["phase 1", "phase 2"] {
            r.project_op(
                n,
                ohl_server::Op::CreateLine {
                    name: n.into(),
                    from: "main".into(),
                },
            );
        }
        r.model.show_line("phase 2").unwrap();
        // A conflict to settle: T3 moved on both lines.
        let p = r.model.line.structure("T3").unwrap().position;
        r.perform(crate::edit::Action::Command {
            what: "T3".into(),
            command: ohl_session::Command::MoveStructure {
                id: "T3".into(),
                position: [p[0] + 3.0, p[1], p[2]],
            },
        });
        r.switch_line("phase 1");
        for k in 0..r.model.cases.len() {
            r.model.ensure_solved(k);
        }
        r.perform(crate::edit::Action::Command {
            what: "T3".into(),
            command: ohl_session::Command::MoveStructure {
                id: "T3".into(),
                position: [p[0] - 3.0, p[1], p[2]],
            },
        });
    }
    harness.run();
    harness.get_by_label("Versions").click();
    harness.run();
    harness.get_by_label_contains("Project lines").hover();
    harness.run();
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/lines-menu.png"))
        .unwrap();
    harness.get_by_label("Project lines…").click();
    harness.run();
    harness.run();
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/lines-window.png"))
        .unwrap();
    harness.state_mut().ready().unwrap().lines_window = Some(LinesWindow::BringIn {
        from: "main".into(),
        choices: Default::default(),
        plan: None,
    });
    harness.run();
    harness.run();
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/lines-bring-in.png"))
        .unwrap();
}

/// A project opens on the line this user worked on, with the lines they
/// showed (remembered on their computer).
#[test]
fn the_line_worked_on_and_those_shown_are_remembered() {
    let mut m = synthetic_project("lines-remembered");
    m.project_op(
        "phase",
        ohl_server::Op::CreateLine {
            name: "phase 1".into(),
            from: "main".into(),
        },
    )
    .unwrap();
    switch(&mut m, "phase 1");
    m.show_line("main").unwrap();
    let prefs = m.view_prefs();
    assert_eq!(prefs.line.as_deref(), Some("phase 1"));
    assert_eq!(prefs.shown_lines, ["main"]);
    let mut sources = m.sources.clone();
    drop(m);
    sources.view = Some(prefs);
    let again = model::load(sources, &|_| {}).unwrap();
    assert_eq!(again.project_line(), "phase 1");
    assert_eq!(again.other_lines.len(), 1);
    assert_eq!(again.other_lines[0].name, "main");
}

/// The working tree: a lane per line, where each started, merges, the
/// lines deleted; opened from Lines › Project lines › Working tree; admins
/// roll a line back from it.
#[test]
fn the_working_tree_shows_the_lines_through_time() {
    use ohl_server::Op;
    let mut m = synthetic_project("tree");
    let shift = |m: &mut model::Model, id: &str, dx: f64| {
        let p = m.line.structure(id).unwrap().position;
        m.apply(
            "shift",
            ohl_session::Command::MoveStructure {
                id: id.into(),
                position: [p[0] + dx, p[1], p[2]],
            },
        )
        .unwrap();
    };
    let op = |m: &mut model::Model, op: Op| {
        m.project_op("op", op).unwrap();
    };
    op(
        &mut m,
        Op::Commit {
            message: "start".into(),
        },
    );
    let start = m.session().unwrap().head_revision().unwrap().unwrap();
    for n in ["phase 1", "phase 2"] {
        op(
            &mut m,
            Op::CreateLine {
                name: n.into(),
                from: "main".into(),
            },
        );
    }
    switch(&mut m, "phase 2");
    shift(&mut m, "T5", 1.0);
    op(
        &mut m,
        Op::Commit {
            message: "phase 2 work".into(),
        },
    );
    switch(&mut m, "phase 1");
    shift(&mut m, "T6", 2.0);
    op(
        &mut m,
        Op::Commit {
            message: "phase 1 work".into(),
        },
    );
    switch(&mut m, "main");
    shift(&mut m, "T2", 1.0);
    op(
        &mut m,
        Op::Commit {
            message: "main fix".into(),
        },
    );
    op(
        &mut m,
        Op::DeleteLine {
            name: "phase 2".into(),
        },
    );
    switch(&mut m, "phase 1");
    op(
        &mut m,
        Op::BringIn {
            from: "main".into(),
            choices: Default::default(),
        },
    );
    shift(&mut m, "T6", 1.0);
    let s = m.session().unwrap();
    let tree = crate::working_tree::Tree::layout(
        &s.revisions().unwrap(),
        &s.lines().unwrap(),
        &[],
        &|l| s.has_uncommitted(l).unwrap(),
    );
    let lanes = crate::working_tree::lanes_by_name(&tree);
    assert_eq!(lanes["main"], ["start", "main fix"]);
    assert_eq!(
        lanes["phase 1"],
        ["phase 1 work", "Bring main into phase 1"]
    );
    assert_eq!(lanes["lines deleted"], ["phase 2 work"]);
    // Time goes right: every parent before its child.
    for n in &tree.nodes {
        for p in &n.revision.parents {
            assert!(tree.nodes[tree.index[p]].col < n.col);
        }
    }
    let merge = tree
        .nodes
        .iter()
        .find(|n| n.revision.parents.len() == 2)
        .unwrap();
    assert_eq!(tree.lanes[merge.lane].name, "phase 1");
    assert!(
        tree.lanes
            .iter()
            .find(|l| l.name == "phase 1")
            .unwrap()
            .uncommitted
    );
    assert!(!tree.lanes[0].uncommitted);

    // From the menu; the admin rolls main back to the start.
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(m, None));
    harness.run();
    harness.get_by_label("Versions").click();
    harness.run();
    harness.get_by_label_contains("Project lines").hover();
    harness.run();
    harness.get_by_label("Working tree…").click();
    harness.run();
    assert!(harness.state_mut().ready().unwrap().working_tree.open);
    {
        let r = harness.state_mut().ready().unwrap();
        r.working_tree.selected = Some(start.clone());
    }
    harness.run();
    harness.get_by_label("Roll back…").click();
    harness.run();
    harness.get_by_label("Roll back").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.session().unwrap();
        let t2 = |line: &str| {
            s.line_named(line)
                .unwrap()
                .structure("T2")
                .unwrap()
                .position[0]
        };
        let first = s.revision_state(&start).unwrap().line().unwrap();
        assert_eq!(
            t2("main"),
            first.structure("T2").unwrap().position[0],
            "{:?}",
            r.status
        );
        assert_eq!(r.model.project_line(), "phase 1", "still on phase 1");
        // A designer sees why not.
        if let Ok(e) = &mut r.model.editing {
            e.session
                .set_user("tester", "ben", ohl_session::Role::Designer)
                .unwrap();
            e.user = "ben".into();
        }
    }
    harness.run();
    assert!(
        harness
            .query_by_label("Rolling a line back: admins only.")
            .is_some()
    );
}

#[test]
#[ignore = "writes an image of the working tree to OHL_SHOTS (a look by eye)"]
fn working_tree_image() {
    use ohl_server::Op;
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let mut m = synthetic_project("tree-shot");
    let shift = |m: &mut model::Model, id: &str, dx: f64| {
        let p = m.line.structure(id).unwrap().position;
        m.apply(
            "shift",
            ohl_session::Command::MoveStructure {
                id: id.into(),
                position: [p[0] + dx, p[1], p[2]],
            },
        )
        .unwrap();
    };
    let op = |m: &mut model::Model, op: Op| {
        m.project_op("op", op).unwrap();
    };
    op(
        &mut m,
        Op::Commit {
            message: "as imported".into(),
        },
    );
    op(
        &mut m,
        Op::Tag {
            name: "IFC rev A".into(),
            at: None,
            note: String::new(),
        },
    );
    for n in ["phase 1", "phase 2", "route B"] {
        op(
            &mut m,
            Op::CreateLine {
                name: n.into(),
                from: "main".into(),
            },
        );
    }
    for (line, id, msg) in [
        ("phase 1", "T6", "T6 for the road"),
        ("main", "T2", "T2 corrected"),
        ("phase 2", "T5", "T5 raised"),
        ("phase 1", "T7", "T7 replaced"),
        ("route B", "T4", "route B start"),
        ("main", "T3", "T3 corrected"),
    ] {
        switch(&mut m, line);
        shift(&mut m, id, 1.0);
        op(
            &mut m,
            Op::Commit {
                message: msg.into(),
            },
        );
    }
    switch(&mut m, "phase 1");
    op(
        &mut m,
        Op::BringIn {
            from: "main".into(),
            choices: Default::default(),
        },
    );
    switch(&mut m, "main");
    op(
        &mut m,
        Op::DeleteLine {
            name: "route B".into(),
        },
    );
    shift(&mut m, "T2", 0.5);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(m, None));
    harness.run();
    harness.state_mut().ready().unwrap().working_tree.open = true;
    harness.run();
    harness.run();
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/working-tree.png"))
        .unwrap();
}

/// Working tree: a new line with only uncommitted work can be clicked (its
/// open dot) and committed there; "New line from it" names the revision in
/// words (its line, when, its message), not by its id.
#[test]
fn a_new_lines_work_is_committed_from_the_tree() {
    use crate::project_lines::LinesWindow;
    use ohl_server::Op;
    let mut m = synthetic_project("tree-work");
    m.project_op(
        "c",
        Op::Commit {
            message: "as imported".into(),
        },
    )
    .unwrap();
    m.project_op(
        "l",
        Op::CreateLine {
            name: "phase 1".into(),
            from: "main".into(),
        },
    )
    .unwrap();
    switch(&mut m, "phase 1");
    let p = m.line.structure("T2").unwrap().position;
    m.apply(
        "T2",
        ohl_session::Command::MoveStructure {
            id: "T2".into(),
            position: [p[0] + 1.0, p[1], p[2]],
        },
    )
    .unwrap();
    let start = m.session().unwrap().head_revision().unwrap().unwrap();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(m, None));
    harness.run();
    harness.state_mut().ready().unwrap().working_tree.open = true;
    harness.run();
    // Its uncommitted work (the open dot of lane 1), committed here.
    harness.state_mut().ready().unwrap().working_tree.work = Some((1, "T2 east".into()));
    harness.run();
    assert!(
        harness
            .query_by_label("phase 1: work not committed yet")
            .is_some()
    );
    harness.get_by_label("Commit").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.session().unwrap();
        assert!(!s.has_uncommitted("phase 1").unwrap(), "{:?}", r.status);
        assert_eq!(
            s.revision(&s.head_revision().unwrap().unwrap())
                .unwrap()
                .message,
            "T2 east"
        );
        // New line from the first revision: shown in words.
        r.working_tree.selected = Some(start.clone());
    }
    harness.run();
    harness.get_by_label("New line from it…").click();
    harness.run();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let Some(LinesWindow::New {
        from,
        at: Some((id, words)),
        ..
    }) = &r.lines_window
    else {
        panic!("{:?}", r.lines_window);
    };
    assert_eq!((from, id), (&start, &start));
    assert!(
        words.starts_with("main at ") && words.ends_with("as imported"),
        "{words}"
    );
    if let Ok(dir) = ohl_core::env::var("OHL_SHOTS") {
        harness
            .render()
            .unwrap()
            .save(format!("{dir}/new-line-from-revision.png"))
            .unwrap();
    }
}

#[test]
#[ignore = "times switching project lines on the project OHL_PROJECT (a copy: it is changed)"]
fn line_switch_timing() {
    let Ok(file) = ohl_core::env::var("OHL_PROJECT") else {
        return;
    };
    let mut m = model::load(
        Sources {
            file: file.into(),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    let names: Vec<String> = m
        .session()
        .unwrap()
        .lines()
        .unwrap()
        .into_iter()
        .map(|l| l.name)
        .collect();
    let other = names
        .iter()
        .find(|n| **n != m.project_line())
        .cloned()
        .unwrap_or_else(|| {
            m.project_op(
                "line",
                ohl_server::Op::CreateLine {
                    name: "timing".into(),
                    from: m.project_line(),
                },
            )
            .unwrap();
            "timing".into()
        });
    let back = m.project_line();
    for k in 0..m.cases.len() {
        m.ensure_solved(k);
    }
    // As before: the libraries read again, every case solved at once.
    let t = std::time::Instant::now();
    m.reload_libraries().unwrap();
    let old = t.elapsed();
    let t = std::time::Instant::now();
    m.switch_line(&other).unwrap();
    let new = t.elapsed();
    let t = std::time::Instant::now();
    m.ensure_solved(0);
    let first = t.elapsed();
    m.switch_line(&back).unwrap();
    println!(
        "{} cases, {} structures: before {old:.2?}; now {new:.2?} to switch, the case shown solved in the background in {first:.2?}",
        m.cases.len(),
        m.line.structures.len()
    );
}

/// Lines › Description of …: a long description edited in a box that
/// scrolls; shown under the working tree; read only for others.
#[test]
fn lines_are_described() {
    use crate::project_lines::LinesWindow;
    let mut m = synthetic_project("described");
    m.project_op(
        "l",
        ohl_server::Op::CreateLine {
            name: "phase 1".into(),
            from: "main".into(),
        },
    )
    .unwrap();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(m, None));
    harness.run();
    harness.get_by_label("Versions").click();
    harness.run();
    harness.get_by_label("Description of main…").click();
    harness.run();
    let long: String = (1..=80)
        .map(|k| format!("Point {k} of the scope.\n"))
        .collect();
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(LinesWindow::Description { line, text }) = &mut r.lines_window else {
            panic!("{:?}", r.lines_window);
        };
        assert_eq!(line, "main");
        *text = long.clone();
    }
    harness.run();
    // The box scrolls: Save stays on the screen.
    let save = harness.get_by_label("Save").rect();
    assert!(save.max.y < 900.0, "{save:?}");
    harness.get_by_label("Save").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.lines_window.is_none(), "{:?}", r.status);
        let lines = r.model.session().unwrap().lines().unwrap();
        assert_eq!(lines[0].description, long.trim_end());
        r.working_tree.open = true;
    }
    harness.run();
    harness.run();
    assert!(harness.query_by_label("Description of main").is_some());
    assert!(
        harness
            .query_by_label_contains("Point 1 of the scope.")
            .is_some()
    );
    if let Ok(dir) = ohl_core::env::var("OHL_SHOTS") {
        harness
            .render()
            .unwrap()
            .save(format!("{dir}/tree-description.png"))
            .unwrap();
    }
    // A designer reads the main line's description, does not edit it.
    {
        let r = harness.state_mut().ready().unwrap();
        if let Ok(e) = &mut r.model.editing {
            e.session
                .set_user("tester", "ben", ohl_session::Role::Designer)
                .unwrap();
            e.user = "ben".into();
        }
        r.working_tree.open = false;
        r.lines_window = Some(LinesWindow::Description {
            line: "main".into(),
            text: long.clone(),
        });
    }
    harness.run();
    assert!(harness.query_by_label_contains("Read only").is_some());
    assert!(harness.query_by_label("Save").is_none());
}

#[test]
#[ignore = "imports OHL_XML with the project plugins of OHL_PLUGINS through the app (local check)"]
fn local_project_import_through_the_app() {
    let (Ok(xml), Ok(plugins), Ok(out)) = (
        ohl_core::env::var("OHL_XML"),
        ohl_core::env::var("OHL_PLUGINS"),
        ohl_core::env::var("OHL_OUT"),
    ) else {
        return;
    };
    let into = PathBuf::from(out);
    std::fs::remove_file(&into).ok();
    let mut app = App::empty(Sources::default(), None);
    app.plugins = crate::plugins::Plugins::load(None, None);
    app.plugins.folders = vec![PathBuf::from(plugins)];
    app.plugins.refresh();
    app.file_action = Some(crate::files::FileAction::ImportProject);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| app);
    h.step();
    h.step();
    h.state_mut()
        .plugins
        .create_project(crate::plugins::ProjectImport {
            path: xml.into(),
            format: None,
            options: Default::default(),
            into: into.clone(),
            user: crate::files::user(),
            clip_in: false,
        });
    let started = std::time::Instant::now();
    loop {
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let app = h.state_mut();
        if let Some(w) = &app.import_project
            && let Some(e) = &w.error
        {
            panic!("import refused: {e}");
        }
        if let Some(r) = app.ready() {
            println!(
                "opened: {} structures, {} sections, {} cases, {} solved",
                r.model.line.structures.len(),
                r.model.line.sections.len(),
                r.model.cases.len(),
                r.model.cases.iter().filter(|c| c.ready).count()
            );
            break;
        }
        if let Some(why) = app.failed() {
            panic!("opening failed: {why}");
        }
        assert!(started.elapsed().as_secs() < 240, "not opened in 240 s");
    }
    // Then a TIN into the new project (OHL_TIN): drawn without reopening.
    let Ok(tin) = ohl_core::env::var("OHL_TIN") else {
        return;
    };
    {
        let app = h.state_mut();
        let mut plugins = std::mem::take(&mut app.plugins);
        let r = app.ready().unwrap();
        r.surfaces.show(&r.model);
        let project = r.model.sources.file.clone();
        let user = crate::files::user();
        r.surfaces.act(
            crate::surfaces::Action::Import(tin.into()),
            &mut r.model,
            &mut plugins,
            &user,
            &project,
            &egui::Context::default(),
        );
        app.plugins = plugins;
    }
    let started = std::time::Instant::now();
    loop {
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let r = h.state_mut().ready().unwrap();
        if r.model.ground_source.starts_with("surface") {
            println!(
                "ground: {} in {:.0?}; shown: {}",
                r.model.ground_source,
                started.elapsed(),
                r.visible.contains(&crate::scene::Layer::Ground)
            );
            break;
        }
        if let Some((text, true)) = &r.surfaces.status {
            panic!("TIN import: {text}");
        }
        assert!(
            started.elapsed().as_secs() < 900,
            "no ground in 900 s: {:?} {:?}",
            r.surfaces.list,
            r.surfaces.status
        );
    }
    if let Ok(dir) = ohl_core::env::var("OHL_SHOTS") {
        h.state_mut().show_tab(crate::app::Tab::View3d);
        for _ in 0..60 {
            h.step();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        h.render()
            .unwrap()
            .save(format!("{dir}/tin-3d.png"))
            .unwrap();
    }
}

#[test]
#[ignore = "writes an image of the Terrain surfaces window to OHL_SHOTS (a look by eye)"]
fn surfaces_sections_image() {
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let mut model = synthetic_project("surfaces-sections");
    model.points = vec![([0, 0, 0], 2), ([1000, 0, 0], 5)];
    model.class_counts = vec![(2, 40), (5, 7)];
    model.cloud = Some("survey.ohl".into());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.surfaces.show(&r.model);
    }
    harness.run();
    harness.run();
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/surfaces-sections.png"))
        .unwrap();
}

/// The window closing closes the project: every connection to the file
/// closed, nothing left beside it (no -wal, no -shm).
#[test]
fn closing_the_window_leaves_the_project_file_whole() {
    let model = synthetic_project("exit-clean");
    let file = model.sources.file.clone();
    let side = |end: &str| {
        let mut p = file.clone().into_os_string();
        p.push(end);
        std::path::PathBuf::from(p)
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // An edit: the log has something to write back.
    {
        let r = harness.state_mut().ready().unwrap();
        let p = r.model.line.structure("T2").unwrap().position;
        r.perform(crate::edit::Action::Command {
            what: "T2".into(),
            command: ohl_session::Command::MoveStructure {
                id: "T2".into(),
                position: [p[0] + 1.0, p[1], p[2]],
            },
        });
    }
    assert!(side("-wal").exists());
    eframe::App::on_exit(harness.state_mut());
    assert!(!side("-wal").exists(), "the log written into the file");
    assert!(!side("-shm").exists());
    // And the edit is in the file.
    let s = ohl_session::Session::open(&file).unwrap();
    assert_eq!(s.line().structure("T2").unwrap().position[0], 311.0);
}

/// A project made again at the same place: this user's remembered ground
/// already names the surface imported. Imported, it is drawn at once (it
/// was only after reopening).
#[test]
fn a_surface_imported_under_the_remembered_name_is_drawn_at_once() {
    use crate::surfaces::Action;
    let mut model = synthetic_project("surfaces-remembered");
    assert!(model.ground.is_none());
    model.surface_settings.active = Some("synthetic-survey".into());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.step();
    harness.state_mut().ready().unwrap().view3d.dirty.clear();
    {
        let app = harness.state_mut();
        let mut plugins = std::mem::take(&mut app.plugins);
        let r = app.ready().unwrap();
        r.surfaces.show(&r.model);
        let project = r.model.sources.file.clone();
        r.surfaces.act(
            Action::Import(root().join("examples/synthetic-line/synthetic-survey.csv")),
            &mut r.model,
            &mut plugins,
            "tester",
            &project,
            &egui::Context::default(),
        );
        app.plugins = plugins;
    }
    run_until(&mut harness, |a| {
        a.ready().unwrap().model.ground_source == "surface synthetic-survey"
    });
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.ground.is_some());
    assert!(r.view3d.dirty.contains(&crate::scene::Layer::Ground));
    assert!(r.visible.contains(&crate::scene::Layer::Ground));
}
