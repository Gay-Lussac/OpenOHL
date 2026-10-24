// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: typed projects and the structure picker, Replace structure,
//! sheets and drawings, project lines shown.

use super::*;

/// The example's typed line as a project, its types (and one more, a copy
/// of type-02 with two extensions, `ext-02`) copied into it.
pub(crate) fn typed_project(name: &str) -> model::Model {
    typed_project_with(name, |_| {})
}

/// [`typed_project`] with its structure files (the folder given) changed
/// first.
pub(crate) fn typed_project_with(name: &str, edit: impl FnOnce(&Path)) -> model::Model {
    typed_project_edited(name, edit, |_| {})
}

/// [`typed_project_with`], its line changed first too (`line`).
pub(crate) fn typed_project_edited(
    name: &str,
    edit: impl FnOnce(&Path),
    line: impl FnOnce(&mut ohl_line::Line),
) -> model::Model {
    let edit_line = line;
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join("Structures")).unwrap();
    for e in std::fs::read_dir(ex.join("Structures")).unwrap() {
        let p = e.unwrap().path();
        std::fs::copy(&p, dir.join("Structures").join(p.file_name().unwrap())).unwrap();
    }
    let text = std::fs::read_to_string(ex.join("Structures/type-02.ohls")).unwrap();
    let body = text
        .lines()
        .skip_while(|l| l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .replacen("id = \"type-02\"", "id = \"ext-02\"", 1)
        .replacen(
            "name = \"type-02 (2 structures, e.g. T2)\"",
            "name = \"Tall suspension tower\"",
            1,
        );
    std::fs::write(
        dir.join("Structures/ext-02.ohls"),
        format!(
            "# A taller copy of type-02 for road crossings.\n{body}\n\n\
             [[extension]]\nname = \"+3\"\ndz = \"3 m\"\n\n\
             [[extension]]\nname = \"+6\"\ndz = \"6 m\"\n"
        ),
    )
    .unwrap();
    edit(&dir.join("Structures"));
    let file = dir.join("p.ohl");
    let mut line = ohl_line::Line::load(&ex.join("synthetic-line-typed.ohl.json")).unwrap();
    edit_line(&mut line);
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

/// The structure picker lists the library's types with what they are,
/// filters on any words of their id, name, kind or description and sorts
/// by a column; the description comes from the notes at the top of the
/// file, the level and what the next one lacks from the content.
#[test]
fn the_structure_picker_lists_filters_and_sorts_the_types() {
    use crate::structure_picker::{Column, Picker, TypeRow};
    let m = typed_project("picker-list");
    let mut p = Picker::new("t", Some("type-02"), None, true, true);
    let all = p.shown_ids(&m, "");
    assert_eq!(all.len(), 7, "{all:?}");
    assert_eq!(all[0], "ext-02");
    assert_eq!(p.shown_ids(&m, "taller ROAD"), ["ext-02"]);
    assert_eq!(p.shown_ids(&m, "suspension tower"), ["ext-02"]);
    assert_eq!(p.shown_ids(&m, "e.g. T3"), ["type-03"]);
    assert!(p.shown_ids(&m, "nothing like this").is_empty());
    // The most used first.
    p.sort = (Column::Used, false);
    assert_eq!(p.shown_ids(&m, "")[0], "type-02");
    let (types, insulators) = m.types.as_ref().unwrap();
    let t = types.get("ext-02").unwrap();
    let row = TypeRow::of(t, insulators, types.path("ext-02"));
    assert_eq!(
        row.description,
        "A taller copy of type-02 for road crossings."
    );
    assert_eq!(row.extensions.len(), 2);
    assert_eq!(row.level, t.detail().level());
    assert!(row.height > 10.0, "{}", row.height);
    let t1 = types.get("type-01").unwrap();
    let row = TypeRow::of(t1, insulators, types.path("type-01"));
    assert_eq!(row.level, 1);
    assert_eq!(row.lacks.len(), 1, "{:?}", row.lacks);
    assert!(row.sets > 0 && row.wire_points >= row.sets);
}

/// Add structure: the type's button opens the structure picker; a filter
/// typed narrows the table, a row clicked shows it, Choose puts it (and
/// the extension ticked) in the window, and the structure is added with
/// that type. Escape closes the picker, not the window (OHL_RENDER_PNG: an
/// image of the picker).
#[test]
fn add_structure_chooses_its_type_in_the_structure_picker() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = typed_project("picker-add");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    crate::structure_picker::Picker::set_filter(&harness.ctx, "");
    let (t4, t5) = {
        let r = harness.state_mut().ready().unwrap();
        let l = &r.model.line;
        (
            l.structure("T4").unwrap().position,
            l.structure("T5").unwrap().position,
        )
    };
    let at = [2.0 * t5[0] - t4[0], 2.0 * t5[1] - t4[1], t5[2]];
    use_tool(&mut harness, Tool::AddStructure, Selection::None, Some(at));
    let button = {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Structure(f)) = &r.editor else {
            panic!("{:?}", r.editor);
        };
        let id = f
            .structure_type
            .clone()
            .expect("the nearest structure's type");
        let name = r
            .model
            .types
            .as_ref()
            .unwrap()
            .0
            .get(&id)
            .unwrap()
            .name
            .clone();
        format!("{id} — {name} …")
    };
    harness.get_by_label(&button).click();
    harness.run();
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .structure_picker
            .is_some()
    );
    harness.get_by_label("Choose");
    // Escape: the picker closes, the structure window stays.
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.structure_picker.is_none());
        assert!(matches!(r.editor, Some(Form::Structure(_))));
    }
    harness.get_by_label(&button).click();
    harness.run();
    // The filter has the keyboard: typed, the table narrows.
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("crossings".into()));
    harness.run();
    assert_eq!(
        crate::structure_picker::Picker::filter(&harness.ctx),
        "crossings"
    );
    harness.get_by_label("1 of 7 types");
    assert!(harness.query_by_label("type-03").is_none());
    harness.get_by_label("ext-02").click();
    harness.run();
    // Its details under the preview; the extension chosen here.
    assert_eq!(
        harness.query_all_by_label("Tall suspension tower").count(),
        2
    );
    assert_eq!(
        harness
            .query_all_by_label("A taller copy of type-02 for road crossings.")
            .count(),
        2
    );
    harness.get_by_label("+6").click();
    harness.run();
    if let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
        && let Ok(image) = harness.render()
    {
        let path = PathBuf::from(dir).join("ohl-design-structure-picker.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    harness.get_by_label("Choose").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.structure_picker.is_none(), "Choose closes it");
        let Some(Form::Structure(f)) = &r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.structure_type.as_deref(), Some("ext-02"));
        assert_eq!(f.extension.as_deref(), Some("+6"));
    }
    harness.get_by_label("ext-02 — Tall suspension tower …");
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.editor.is_none(), "{:?}", r.status);
    let s = r.model.line.structure("T8").unwrap();
    assert_eq!(s.structure_type.as_deref(), Some("ext-02"));
    assert_eq!(s.extension.as_deref(), Some("+6"));
    // The filter is remembered for the session.
    assert_eq!(
        crate::structure_picker::Picker::filter(&harness.ctx),
        "crossings"
    );
}

/// The Structures table's type cell opens the structure picker; the type
/// (and extension) chosen are pending edits of the row, applied as one
/// change that one undo takes back.
#[test]
fn the_structures_table_chooses_a_type_in_the_structure_picker() {
    use crate::tables::{TableKind, TypeCell};
    let model = typed_project("picker-table");
    // Short steps: two clicks a double-click.
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 1000.0))
        .with_step_dt(0.02)
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    crate::structure_picker::Picker::set_filter(&harness.ctx, "");
    harness.state_mut().ready().unwrap().open_page =
        Some((crate::app::Tab::Table(TableKind::Structures), None));
    harness.run();
    harness.run();
    // T1 alone has type-01: its cell opens the picker on it.
    harness.get_by_label("type-01 …").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let (cell, picker) = r.tables[&TableKind::Structures]
            .type_picker
            .as_ref()
            .expect("the picker");
        assert!(matches!(cell, TypeCell::Row { key, .. } if key == "T1"));
        assert_eq!(picker.selected.as_deref(), Some("type-01"));
    }
    harness.get_by_label("Cancel").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let page = &r.tables[&TableKind::Structures];
        assert!(page.type_picker.is_none(), "Cancel closes it");
        assert_eq!(page.pending(), 0, "{:?}", page.edits);
    }
    // T4 (type-02): ext-02 with +3, chosen by a double-click on its row.
    {
        let r = harness.state_mut().ready().unwrap();
        r.tables
            .get_mut(&TableKind::Structures)
            .unwrap()
            .open_type_picker(
                TypeCell::Row {
                    key: "T4".into(),
                    structure_type: "type-02".into(),
                    extension: String::new(),
                },
                "type-02",
                "",
            );
    }
    harness.run();
    harness.get_by_label("ext-02").click();
    harness.run();
    harness.get_by_label("+3").click();
    harness.run();
    harness.get_by_label("ext-02").click();
    harness.get_by_label("ext-02").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let page = &r.tables[&TableKind::Structures];
        assert!(
            page.type_picker.is_none(),
            "a double-click chooses: {:?} {:?}",
            page.type_picker
                .as_ref()
                .map(|p| (&p.1.selected, &p.1.extension)),
            page.edits
        );
        let e = &page.edits["T4"];
        assert_eq!(e[&TableKind::Structures.column("type")], "ext-02");
        assert_eq!(e[&TableKind::Structures.column("extension")], "+3");
    }
    apply_table(&mut harness, TableKind::Structures, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.line.structure("T4").unwrap();
        assert_eq!(s.structure_type.as_deref(), Some("ext-02"));
        assert_eq!(s.extension.as_deref(), Some("+3"));
    }
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    let r = harness.state_mut().ready().unwrap();
    let s = r.model.line.structure("T4").unwrap();
    assert_eq!(s.structure_type.as_deref(), Some("type-02"), "one undo");
    assert_eq!(s.extension, None);
}

/// Replace structure: the tool on T2 (type-02, sets 1 and 11) opens the
/// structure picker; type-04 has both sets and a joint at each of its
/// attachment points, so Choose replaces it at once and the status line
/// says how many wires were rewired; one undo takes it back.
#[test]
fn replace_structure_with_every_set_matching_rewires_at_once() {
    use crate::tools::Tool;
    let model = typed_project("replace-match");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    crate::structure_picker::Picker::set_filter(&harness.ctx, "type-04");
    use_tool(
        &mut harness,
        Tool::Replace,
        Selection::Structure("T2".into()),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let rep = r.replace.as_ref().expect("the picker");
        let crate::replace::Stage::Pick(p) = &rep.stage else {
            panic!("the picker first");
        };
        assert_eq!(p.selected.as_deref(), Some("type-02"), "its type");
    }
    harness.get_by_label("type-04").click();
    harness.run();
    harness.get_by_label("Choose").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.replace.is_none(), "no mapping needed");
    let (status, error) = r.status.clone().unwrap();
    assert!(!error && status.contains("rewired 2 wires"), "{status}");
    let s = r.model.line.structure("T2").unwrap();
    assert_eq!(s.structure_type.as_deref(), Some("type-04"));
    // Resolved from its new type: its points linked there.
    assert!(s.points.iter().all(ohl_line::StructurePoint::is_linked));
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    let r = harness.state_mut().ready().unwrap();
    let s = r.model.line.structure("T2").unwrap();
    assert_eq!(s.structure_type.as_deref(), Some("type-02"), "one undo");
}

/// Replace structure on T3 (type-03, sets 1, 11 and 2) by type-02 (sets
/// 1 and 11), from its Properties' action: the mapping window lists the
/// old wire points with what ends there, proposes none for set 2, lists
/// S3 and its jumper as removed (drawn red in the 3D view); Escape
/// changes nothing; Replace removes them as one undoable change.
#[test]
fn replace_structure_maps_wire_points_and_lists_what_goes() {
    use crate::replace::Stage;
    let model = typed_project("replace-map");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    crate::structure_picker::Picker::set_filter(&harness.ctx, "type-02");
    let open = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.perform(crate::edit::Action::ReplaceStructure { id: "T3".into() });
        h.run();
        h.get_by_label("type-02").click();
        h.run();
        h.get_by_label("Choose").click();
        h.run();
    };
    open(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Stage::Map(m)) = r.replace.as_ref().map(|x| &x.stage) else {
            panic!("the mapping window: {:?}", r.status);
        };
        assert_eq!(
            m.choice.iter().map(|(a, b)| (*a, *b)).collect::<Vec<_>>(),
            vec![(1, Some(1)), (2, None), (11, Some(11))]
        );
        let i = m.impact.as_ref().unwrap();
        assert_eq!(i.sections, vec!["S3"]);
        assert_eq!(i.jumpers, vec!["J-T3-branch"]);
        assert!(!r.view3d.removed.is_empty(), "drawn red");
    }
    harness.get_by_label("Replace structure T3 by type-02");
    harness.get_by_label("S3, jumper J-T3-branch");
    harness.get_by_label("  S3");
    if let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
        && let Ok(image) = harness.render()
    {
        let path = PathBuf::from(dir).join("ohl-design-replace-structure.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    // Escape: closed, nothing changed, nothing drawn red.
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.replace.is_none());
        assert!(r.view3d.removed.is_empty());
        assert!(r.model.line.sections.iter().any(|s| s.id == "S3"));
    }
    open(&mut harness);
    // Its attachment points 2.5 cm off type-02's joints (A2 far from
    // any): Replace waits until each is chosen — those type-02 has by
    // label as it has them, A2 removed.
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Stage::Map(m)) = r.replace.as_mut().map(|x| &mut x.stage) else {
            panic!("the mapping window");
        };
        assert!(!m.points_chosen());
        for (k, row) in m.points.clone().iter().enumerate() {
            m.point_choice[k] = Some(match &row.by_type {
                Some((joint, offset)) => ohl_line::links::PointChoice::Link {
                    joint: joint.clone(),
                    offset: *offset,
                },
                None => ohl_line::links::PointChoice::Remove,
            });
        }
    }
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Stage::Map(m)) = r.replace.as_ref().map(|x| &x.stage) else {
            panic!("the mapping window");
        };
        assert!(m.impact.is_ok(), "{:?}", m.impact);
        assert!(m.points_chosen(), "{:?}", m.point_choice);
    }
    harness.get_by_label("Replace").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.replace.is_none(), "{:?}", r.status);
        let l = &r.model.line;
        assert_eq!(
            l.structure("T3").unwrap().structure_type.as_deref(),
            Some("type-02")
        );
        assert!(l.sections.iter().all(|s| s.id != "S3"));
        assert!(l.jumpers.is_empty());
        let (status, _) = r.status.clone().unwrap();
        assert!(status.contains("rewired 2 wires"), "{status}");
    }
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    let r = harness.state_mut().ready().unwrap();
    assert!(
        r.model.line.sections.iter().any(|s| s.id == "S3"),
        "one undo"
    );
    assert_eq!(r.model.line.jumpers.len(), 1);
}

/// A section that stops solving (after creep, with no creep case) is
/// drawn in each case in its last solved shape, dashed red, and a click
/// on that copy picks its span (the copy's case remembered); a section
/// never solved anywhere is drawn as the straight chords.
#[test]
fn a_section_not_solved_is_drawn_dashed_where_it_was() {
    use crate::scene::{Layer, UNSOLVED_COLOR, prims};
    use crate::selection::{Snaps, pick_in_model_case};
    use ohl_line::condition::Calculation;
    let mut m = synthetic();
    let before = m.cases[0]
        .shapes
        .iter()
        .find(|s| s.section == "S1")
        .unwrap()
        .clone();
    m.line.sections[0].calculation = Calculation::CREEP_RS;
    let (line, library) = (m.line.clone(), m.library.clone());
    for c in &mut m.cases {
        c.update(&line, &library, &["S1".to_owned()]);
    }
    let c = &m.cases[0];
    assert!(c.shapes.iter().all(|s| s.section != "S1"));
    assert_eq!(c.unsolved.len(), 1, "{:?}", c.unsolved);
    assert_eq!(c.unsolved[0].0, "S1");
    let d = m.unsolved_drawn("S1", 0, 0).unwrap();
    assert!(d.from.contains("last solved"), "{}", d.from);
    assert!(d.reason.contains("Creep-Stretch"), "{}", d.reason);
    assert_eq!(d.spans[0], before.spans[0].points);
    // Drawn: red dashes (more pieces than the span has points apart).
    let red: Vec<_> = prims(&m, Layer::Wires, 0)
        .into_iter()
        .filter(|p| p.color[..3] == UNSOLVED_COLOR)
        .collect();
    assert!(red.len() > before.spans.len() * 2, "{}", red.len());
    // Solved ones are not.
    assert!(m.unsolved_drawn("S2", 0, 0).is_none());
    // A click on the dashed copy (seen from above) picks its span.
    let pts = &d.spans[1];
    let mid = pts[pts.len() / 2];
    let (picked, _) = pick_in_model_case(&m, 0, &Snaps::default(), [mid[0], mid[1]], 0.5, |p| {
        Some([p[0], p[1]])
    });
    assert_eq!(
        picked,
        Selection::Span {
            section: "S1".into(),
            span: 1
        }
    );
    // Shown in two cases: each copy drawn and picked, with its case.
    m.shown_cases = vec![0, 1];
    let d1 = m.unsolved_drawn("S1", 1, 0).unwrap();
    let mid1 = d1.spans[1][d1.spans[1].len() / 2];
    let (picked, k) = pick_in_model_case(&m, 0, &Snaps::default(), [mid1[0], mid1[1]], 0.05, |p| {
        Some([p[0], p[1]])
    });
    assert!(matches!(picked, Selection::Span { ref section, .. } if section == "S1"));
    if d1.spans[1] != d.spans[1] {
        assert_eq!(k, Some(1));
    }
    // Solved from scratch in every case (nothing before): the chords.
    for k in 0..m.cases.len() {
        m.cases[k] = model::Solved::new(&m.line, &m.library, m.cases[k].case.clone());
    }
    let d = m.unsolved_drawn("S1", 0, 0).unwrap();
    assert!(d.from.contains("chords"), "{}", d.from);
    assert!(d.spans.iter().all(|s| s.len() == 2));
}

/// Checks › Structure loads with the ice case (after creep) not solved:
/// the Usage page shows the structures as "not checked" in ice (grey,
/// the sections not solved named), and coloured by usage they are
/// marked so in the views.
#[test]
fn the_usage_page_shows_structures_not_checked() {
    use crate::loads::UsageMark;
    let model = synthetic_project("incomplete");
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
    {
        let r = harness.state_mut().ready().unwrap();
        let results = r.loads.results.as_ref().unwrap().as_ref().unwrap();
        let ice = results.iter().position(|c| c.case.name == "ice").unwrap();
        assert!(
            results[ice].is_incomplete("T1"),
            "{:?}",
            results[ice].failed
        );
        assert!(!results[0].is_incomplete("T1"));
        // The usage of ice: T1 incomplete, the everyday case's not.
        assert!(
            r.loads.usage[ice]
                .structures
                .iter()
                .all(|s| s.is_incomplete())
        );
        assert!(
            r.loads.usage[0]
                .structures
                .iter()
                .all(|s| !s.is_incomplete())
        );
        r.loads.show_usage = true;
        r.loads.usage_case = ice + 1;
    }
    harness.step();
    harness.step();
    assert!(harness.query_all_by_label("not checked").next().is_some());
    assert!(
        harness
            .query_all_by_label_contains("not checked: S1 not solved, S2 not solved in ice")
            .next()
            .is_some()
    );
    // Coloured by usage: incomplete, the legend's neutral colour.
    harness.get_by_label("Colour structures by usage").click();
    harness.step();
    harness.step();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.loads.colour_structures);
    assert_eq!(r.model.usage_marks.get("T1"), Some(&UsageMark::Incomplete));
    assert_eq!(
        UsageMark::Incomplete.label(),
        "not checked: a wire not solved"
    );
}

/// The sheet preview: off by default; ticked in the Profile page's Sheets
/// menu (which stays open), it shows the export's sheets over the profile
/// and the plan — exactly those File › Export › Drawings… writes, warnings
/// included — and follows the export settings as they change.
#[test]
fn the_sheet_preview_shows_the_exports_sheets_over_the_profile_and_plan() {
    let model = synthetic();
    // On the GPU when there is one (images with OHL_RENDER_PNG).
    let mut builder = Harness::builder().with_size(egui::vec2(1400.0, 900.0));
    if ohl_render::headless_device().is_ok() {
        builder = builder.wgpu();
    }
    let mut harness = builder.build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().show_tab(crate::app::Tab::Profile);
    harness.run_steps(3);
    // Off by default: nothing drawn, no preview made.
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(!r.drawings.preview_on);
        assert!(r.profile.sheets_drawn.is_empty());
    }
    assert!(harness.query_by_label_contains("Sheet preview:").is_none());
    // Ticked in the Profile page's Sheets menu: the menu stays open.
    harness.get_by_label("Sheets").click();
    harness.run();
    harness.get_by_label(crate::drawings::PREVIEW_TICK).click();
    harness.run_steps(3);
    harness.get_by_label(crate::drawings::PREVIEW_TICK);
    assert!(harness.state_mut().ready().unwrap().drawings.preview_on);
    crate::ux_tests::shot(&mut harness, "sheet-preview-profile");
    // The sheets of the alignments shown, as the export lays them out (A3
    // at 1:2000 / 1:200 by default).
    let (shown, case) = {
        let r = harness.state_mut().ready().unwrap();
        let shown: Vec<(String, usize, f64, f64, bool)> = r.profile.sheets_drawn.clone();
        (shown, r.case)
    };
    assert!(shown.len() >= 2, "{shown:?}");
    let id = shown[0].0.clone();
    harness.get_by_label_contains(&format!("Sheet preview: {} sheet(s) A3", shown.len()));
    // The export writes the same sheets, with the same warnings.
    let dir = std::env::temp_dir().join(format!("ohl-app-sheet-preview-{}", std::process::id()));
    let (written, preview) = {
        let r = harness.state_mut().ready().unwrap();
        let routes = r.model.line.alignment_views();
        let k = routes.iter().position(|a| a.id == id).unwrap();
        let w = crate::drawings::DrawingsWindow {
            route: Some(k),
            formats: vec![ohl_drawing::export::Format::Svg],
            out: Some(dir.clone()),
            ..r.drawings.settings().clone()
        };
        let input = crate::drawings::Input::new(&r.model, case, &w).unwrap();
        let report = crate::drawings::run(&input, &|_, _| {}).unwrap();
        let preview = crate::drawings::preview(&r.model, case, &w).unwrap();
        (report, preview)
    };
    let route = preview.route(&id).unwrap();
    let sheet_files = std::fs::read_dir(&dir)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("-sheet-")
        })
        .count();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(sheet_files, route.sheets.len());
    let profile_sheets: Vec<_> = shown.iter().filter(|s| s.0 == id).collect();
    assert_eq!(profile_sheets.len(), route.sheets.len());
    for (s, l) in profile_sheets.iter().zip(&route.sheets) {
        // (One alignment shown first: no shift.)
        assert_eq!((s.1, s.4), (l.number, !l.warnings.is_empty()));
    }
    let exported: Vec<&str> = written
        .lines()
        .filter_map(|l| l.strip_prefix("⚠ "))
        .collect();
    let previewed: Vec<String> = preview
        .warnings()
        .into_iter()
        .filter(|w| w.starts_with(&format!("route {id}: ")))
        .collect();
    assert_eq!(exported, previewed);
    // The settings change (A4, window closed: those it had): the
    // preview follows.
    harness.state_mut().ready().unwrap().drawings.last.page = "A4".into();
    // (Drawn again once the settings have settled.)
    harness.run_steps(1);
    std::thread::sleep(crate::drawings::PREVIEW_SETTLE);
    harness.run_steps(3);
    let a4 = harness
        .state_mut()
        .ready()
        .unwrap()
        .profile
        .sheets_drawn
        .iter()
        .filter(|s| s.0 == id)
        .count();
    assert!(a4 > profile_sheets.len(), "{a4}");
    // The window opened again starts from the settings kept.
    harness.state_mut().ready().unwrap().drawings.open();
    harness.run();
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .drawings
            .window
            .as_ref()
            .unwrap()
            .page,
        "A4"
    );
    // The plan shows them too.
    harness.state_mut().show_tab(crate::app::Tab::Plan);
    harness.run_steps(3);
    assert!(harness.state_mut().ready().unwrap().plan.sheets_drawn > 0);
    crate::ux_tests::shot(&mut harness, "sheet-preview-plan");
    // One continuous drawing: no sheets, said so.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .drawings
        .window
        .as_mut()
        .unwrap()
        .page = String::new();
    harness.state_mut().show_tab(crate::app::Tab::Profile);
    harness.run_steps(1);
    std::thread::sleep(crate::drawings::PREVIEW_SETTLE);
    harness.run_steps(3);
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .profile
            .sheets_drawn
            .is_empty()
    );
    harness.get_by_label_contains("the export writes one continuous drawing");
}

/// File › Export › Drawings…: the line width and the break clearance are
/// export settings, checked before anything is written.
#[test]
fn the_export_window_takes_the_line_width_and_break_clearance() {
    let m = synthetic();
    let w = crate::drawings::DrawingsWindow {
        out: Some(std::env::temp_dir()),
        line_width: 25.0,
        break_clearance: 3.5,
        ..Default::default()
    };
    let input = crate::drawings::Input::new(&m, 0, &w).unwrap();
    assert_eq!(input.options.plan_half_width, 25.0);
    assert_eq!(input.options.break_clearance, 3.5);
    for bad in [
        crate::drawings::DrawingsWindow {
            break_clearance: 25.0,
            ..w.clone()
        },
        crate::drawings::DrawingsWindow {
            break_clearance: -1.0,
            ..w.clone()
        },
        crate::drawings::DrawingsWindow {
            line_width: 0.0,
            ..w.clone()
        },
    ] {
        assert!(crate::drawings::Input::new(&m, 0, &bad).is_err());
    }
    // The defaults: 2 mm, ±40 m.
    let d = crate::drawings::DrawingsWindow::default();
    assert_eq!((d.break_clearance, d.line_width), (2.0, 40.0));
}

/// File › Export › Drawings… with a sheet template: a template file loaded
/// is kept in the project (its images embedded) and chosen; its page
/// taken; a required field left empty refuses the export, naming it;
/// filled, the sheets carry it; another project line is drawn in its
/// role; the settings (template, fields, revisions, versions) are kept;
/// a sheet is previewed as the export draws it.
#[test]
fn drawings_take_a_sheet_template_its_fields_and_versions() {
    use crate::drawings::{DrawingSettings, DrawingsWindow, Input};
    use crate::sheet_ui::VersionChoice;
    let mut m = synthetic_project("sheet-template");
    let user = m.editing.as_ref().unwrap().user.clone();
    m.editing
        .as_mut()
        .unwrap()
        .session
        .create_line(&user, "as built", "main")
        .unwrap();
    // The example template, kept in the project.
    let file = root().join("examples/sheet-templates/example-a1.ohlsheet");
    let t = ohl_drawing::SheetTemplate::load(&file).unwrap();
    m.project_op(
        "sheet template",
        ohl_server::Op::PutLibrary {
            kind: crate::sheet_ui::KIND.into(),
            name: "example-a1.ohlsheet".into(),
            text: t.embedded(),
        },
    )
    .unwrap();
    let offers = crate::sheet_ui::offers(&m, &Default::default());
    assert!(offers.iter().any(|o| o.key == "project:example-a1" && o.ok));
    let dir = std::env::temp_dir().join(format!("ohl-app-sheet-template-{}", std::process::id()));
    let mut w = DrawingsWindow {
        route: Some(0),
        formats: vec![ohl_drawing::export::Format::Svg],
        page: "template".into(),
        all_cases: true,
        out: Some(dir.clone()),
        ..DrawingsWindow::default()
    };
    w.sheet.template = "project:example-a1".into();
    w.sheet.versions.push(VersionChoice {
        line: "as built".into(),
        role: "existing".into(),
        label: "{name}".into(),
    });
    w.sheet.role = "project".into();
    // Without the project number: refused, named.
    let input = Input::new(&m, 0, &w).unwrap();
    assert_eq!(input.export.page.as_ref().unwrap().name, "A1");
    let err = crate::drawings::run(&input, &|_, _| {}).unwrap_err();
    assert!(err.contains("Project number (project_no)"), "{err}");
    w.sheet
        .fields
        .values
        .insert("project_no".into(), "P-42".into());
    w.sheet.fields.revisions.push(ohl_drawing::Revision::parse(
        "A;2026-10-20;first issue;AB;CD",
    ));
    let input = Input::new(&m, 0, &w).unwrap();
    assert_eq!(input.versions.len(), 1);
    let report = crate::drawings::run(&input, &|_, _| {}).unwrap();
    assert!(report.contains("files written"), "{report}");
    let svg = std::fs::read_to_string(dir.join("main-sheet-01.svg")).unwrap();
    assert!(svg.contains("P-42-main-001") && svg.contains("first issue"));
    assert!(
        svg.contains("as built: existing"),
        "the version in the legend"
    );
    assert!(svg.contains("OHL_VERSIONS") || svg.contains("as built"));
    // `{height_datum}`: none given, the template's text for an empty
    // field; the project's height datum when given (2026-10-21).
    assert!(
        svg.contains("(not given). Coordinates"),
        "the [if_empty] text"
    );
    m.line.height_datum = Some(ohl_line::HeightDatum {
        name: "LN02".into(),
        epsg: Some(5728),
    });
    let input = Input::new(&m, 0, &w).unwrap();
    crate::drawings::run(&input, &|_, _| {}).unwrap();
    let svg = std::fs::read_to_string(dir.join("main-sheet-01.svg")).unwrap();
    assert!(svg.contains("Heights above LN02 (EPSG:5728). Coordinates"));
    m.line.height_datum = None;
    let _ = std::fs::remove_dir_all(&dir);
    // The lines below the lowest conductor, from the ground rules: in the
    // sheet preview over the profile too (a rule naming no case: the
    // largest sag drawn).
    w.ground_rules = std::sync::Arc::new(
        ohl_clearance::RuleSet::parse(
            "format = \"ohl-clearance-rules\"\nversion = 1\n[[rule]]\nname = \"G\"\ntarget = \"ground\"\nvertical = \"6 m\"",
        )
        .unwrap()
        .rules,
    );
    w.sheet.conductor_clearance = true;
    let p = crate::drawings::preview(&m, 0, &w).unwrap();
    assert!(!p.routes[0].curves.is_empty());
    assert!(
        p.routes[0].curves[0]
            .label
            .starts_with("6 m below the lowest conductor")
    );
    // Kept with the drawing settings.
    let text = serde_json::to_string(&DrawingSettings::of(&w, false)).unwrap();
    let back: DrawingSettings = serde_json::from_str(&text).unwrap();
    let mut again = DrawingsWindow::default();
    back.apply(&mut again);
    assert_eq!(again.sheet, w.sheet);
    assert_eq!(again.page, "template");
    // A sheet previewed in the window, as the export draws it.
    let ctx = egui::Context::default();
    let mut d = crate::drawings::Drawings::default();
    w.sheet_preview = Some(1);
    d.window = Some(w);
    // Drawn in the background: frames until it is there (exit clause: a
    // minute).
    let shown = |d: &mut crate::drawings::Drawings, m: &mut model::Model| {
        let start = std::time::Instant::now();
        loop {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| d.show(ui.ctx(), m, 0));
            out.textures_delta.clear();
            if !d.sheet_drawing() {
                break d.sheet_previewed().unwrap().unwrap();
            }
            assert!(
                start.elapsed().as_secs() < 60,
                "the sheet preview took over a minute"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    };
    let (sheets, warnings, number) = shown(&mut d, &mut m);
    assert!(sheets >= 1, "{warnings:?}");
    assert_eq!(number, 1);
    // A sheet past the route's last: the last shown (not an empty window).
    d.window.as_mut().unwrap().sheet_preview = Some(99);
    let (again, _, number) = shown(&mut d, &mut m);
    assert_eq!((again, number), (sheets, sheets));
}

/// The drawing settings are kept in the project, per project line: a
/// project without them opens with the defaults (±40 m, 2 mm, A3, the
/// preview off); changed (in the window or the profile), they are
/// written as a project setting — not into the undo log — and are there
/// when the project is opened again; another project line has its own.
#[test]
fn the_drawing_settings_are_kept_in_the_project_per_line() {
    use crate::drawings::{DrawingSettings, Drawings, setting_key};
    let model = synthetic_project("drawing-settings");
    let sources = model.sources.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // An old project: the defaults.
    let seq = {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.drawings.stored(), DrawingSettings::default());
        let d = r.drawings.settings();
        assert_eq!(
            (d.line_width, d.break_clearance, d.page.as_str()),
            (40.0, 2.0, "A3")
        );
        assert!(!r.drawings.preview_on);
        assert_eq!(
            crate::drawings::DEFAULT_LINE_WIDTH,
            ohl_drawing::Options::default().plan_half_width
        );
        let s = r.model.session().unwrap();
        assert_eq!(s.setting(&setting_key("main")).unwrap(), None);
        s.last_seq().unwrap()
    };
    // Changed: written into the project (the window closed, then the
    // line width from the profile).
    {
        let r = harness.state_mut().ready().unwrap();
        let w = &mut r.drawings.last;
        w.page = "A1".into();
        w.hscale = 1000.0;
        w.vscale = 100.0;
        w.break_clearance = 3.5;
        w.profile_only = true;
        w.formats = vec![
            ohl_drawing::export::Format::Dxf,
            ohl_drawing::export::Format::Csv,
        ];
        r.drawings.preview_on = true;
        r.drawings.set_line_width(25.0);
    }
    harness.run_steps(2);
    let kept = {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.session().unwrap();
        let text = s.setting(&setting_key("main")).unwrap().unwrap();
        // A setting, not an edit: nothing to undo.
        assert_eq!(s.last_seq().unwrap(), seq);
        serde_json::from_str::<DrawingSettings>(&text).unwrap()
    };
    assert_eq!(
        (
            kept.page.as_str(),
            kept.hscale,
            kept.vscale,
            kept.line_width,
            kept.break_clearance,
            kept.profile_only,
            kept.preview
        ),
        ("A1", 1000.0, 100.0, 25.0, 3.5, true, true)
    );
    assert_eq!(kept.formats, ["dxf", "csv"]);
    // Opened again: there.
    let again = model::load(sources.clone(), &|_| {}).unwrap();
    let d = Drawings::loaded(&again);
    assert_eq!(d.stored(), kept);
    assert_eq!(d.settings().line_width, 25.0);
    assert!(d.preview_on);
    drop(again);
    // Another project line has its own (the defaults at first); back on
    // main, main's.
    {
        let r = harness.state_mut().ready().unwrap();
        r.project_op(
            "phase",
            ohl_server::Op::CreateLine {
                name: "phase 1".into(),
                from: "main".into(),
            },
        );
        r.switch_line("phase 1");
    }
    harness.run_steps(2);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.session().unwrap().current_line(), "phase 1");
        assert_eq!(r.drawings.stored(), DrawingSettings::default());
        r.drawings.set_line_width(60.0);
    }
    harness.run_steps(2);
    {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.session().unwrap();
        assert!(s.setting(&setting_key("phase 1")).unwrap().is_some());
        r.switch_line("main");
    }
    harness.run_steps(2);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.drawings.stored(), kept);
        assert_eq!(r.drawings.line_width(), 25.0);
    }
    // Values out of range (a file edited by hand) take the defaults.
    let odd = DrawingSettings {
        line_width: -5.0,
        break_clearance: 50.0,
        page: "B9".into(),
        hscale: f64::NAN,
        formats: vec!["svg".into(), "doc".into()],
        ..DrawingSettings::default()
    };
    let mut w = crate::drawings::DrawingsWindow::default();
    odd.apply(&mut w);
    assert_eq!(
        (w.line_width, w.break_clearance, w.page.as_str(), w.hscale),
        (40.0, 2.0, "A3", 2000.0)
    );
    assert_eq!(w.formats, [ohl_drawing::export::Format::Svg]);
}

/// Each circuit drawn in a weather case of its own: the earth wires in
/// another case than the one shown take that case's shapes; a stringing
/// case gives the stringing band its sections; a case no name or
/// temperature matches is an error naming it.
#[test]
fn each_circuit_is_drawn_in_its_own_weather_case() {
    let m = synthetic();
    assert!(m.cases.len() >= 2 && m.cases.iter().all(|c| c.ready));
    let other = &m.cases[1];
    let earth: Vec<&str> = m
        .line
        .sections
        .iter()
        .filter(|s| s.system.is_none())
        .map(|s| s.id.as_str())
        .collect();
    assert!(!earth.is_empty());
    let mut w = crate::drawings::DrawingsWindow {
        route: Some(0),
        out: Some(std::env::temp_dir()),
        ..Default::default()
    };
    w.sheet.circuit_cases.insert(
        String::new(),
        crate::sheet_ui::CircuitCases {
            drawn: Some(other.case.name.clone()),
            stringing: Some(other.case.name.clone()),
        },
    );
    let input = crate::drawings::Input::new(&m, 0, &w).unwrap();
    let (name, shapes, _) = &input.cases[0];
    assert!(name.contains(&other.case.name), "{name}");
    for id in &earth {
        let drawn = shapes.iter().find(|s| s.section == *id).unwrap();
        let want = other.shapes.iter().find(|s| s.section == *id).unwrap();
        assert_eq!(drawn, want, "{id}");
    }
    let (case, stringing) = input.stringing.as_ref().unwrap();
    assert_eq!(case, &other.case.name);
    assert_eq!(stringing.len(), earth.len());
    // No such case: refused, named.
    w.sheet.circuit_cases.get_mut("").unwrap().drawn = Some("no such case".into());
    let e = crate::drawings::Input::new(&m, 0, &w).err().unwrap();
    assert!(e.contains("no such case"), "{e}");
}

/// A project line shown is solved from the line worked on's solutions,
/// only what differs solved again, in the background: a line just
/// started differs in nothing; one with a structure moved differs in it
/// and its sections, and its solution in each case equals a full solve.
#[test]
fn a_project_line_shown_shares_the_solutions_of_the_line_worked_on() {
    let mut m = synthetic_project("lines-shared");
    m.project_op(
        "phase",
        ohl_server::Op::CreateLine {
            name: "phase 1".into(),
            from: "main".into(),
        },
    )
    .unwrap();
    m.show_line("phase 1").unwrap();
    let o = &m.other_lines[0];
    assert_eq!(o.differs, model::Differences::default());
    assert!(o.cases.iter().all(|c| !c.ready), "solved when wanted only");
    // Moved on phase 1: it differs there.
    switch(&mut m, "phase 1");
    let id = m.line.structures[1].id.clone();
    let p = m.line.structure(&id).unwrap().position;
    m.apply(
        "shift",
        ohl_session::Command::MoveStructure {
            id: id.clone(),
            position: [p[0] + 3.0, p[1], p[2]],
        },
    )
    .unwrap();
    switch(&mut m, "main");
    m.other_lines.clear();
    m.show_line("phase 1").unwrap();
    let ctx = egui::Context::default();
    let all: Vec<usize> = (0..m.cases.len()).collect();
    for k in &all {
        m.ensure_solved(*k);
    }
    m.want_others(&all, &ctx);
    let start = std::time::Instant::now();
    while m.other_lines[0].cases.iter().any(|c| !c.ready) {
        m.poll_others();
        assert!(start.elapsed().as_secs() < 60, "not solved in a minute");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let o = &m.other_lines[0];
    assert!(o.differs.structures.contains(&id));
    let through: Vec<&String> = o
        .line
        .sections
        .iter()
        .filter(|s| s.points.iter().any(|p| p.structure == id))
        .map(|s| &s.id)
        .collect();
    assert!(!through.is_empty());
    assert!(through.iter().all(|s| o.differs.sections.contains(*s)));
    for (k, c) in o.cases.iter().enumerate() {
        let full = model::Solved::new(&o.line, &m.library, m.cases[k].case.clone());
        assert_eq!(c.shapes, full.shapes, "{}", c.case.name);
        assert_eq!(c.jumpers, full.jumpers, "{}", c.case.name);
    }
    // Drawn: only what differs, unless whole lines are asked for.
    let part = o.differs.of_line(&o.line);
    assert_eq!(part.sections.len(), o.differs.sections.len());
    assert!(part.sections.len() < o.line.sections.len());
}

/// A project line shown whose library files differ from the line worked
/// on's is read and solved with its own (its library changes tracked with
/// it): a wire type made heavier on it makes its sections on that type
/// differ, solved with its data (as a full solve with its library).
#[test]
fn a_project_line_shown_uses_its_own_library_files() {
    let mut m = synthetic_project("lines-own-library");
    m.project_op(
        "phase",
        ohl_server::Op::CreateLine {
            name: "phase 1".into(),
            from: "main".into(),
        },
    )
    .unwrap();
    switch(&mut m, "phase 1");
    let session = &mut m.editing.as_mut().unwrap().session;
    let (file, text) = session
        .library_files("cables")
        .unwrap()
        .into_iter()
        .find(|(_, t)| t.contains("acsr-drake-26-7"))
        .unwrap();
    let heavier = text.replacen(
        "mass_per_length = \"1.094 lbm/ft\"",
        "mass_per_length = \"1.5 lbm/ft\"",
        1,
    );
    assert_ne!(heavier, text);
    session
        .put_library_file("tester", "cables", &file, &heavier)
        .unwrap();
    switch(&mut m, "main");
    let msg = m.show_line("phase 1").unwrap();
    assert!(msg.contains("its own library files: wires"), "{msg}");
    let o = &m.other_lines[0];
    let on_drake: Vec<&String> = o
        .line
        .sections
        .iter()
        .filter(|s| s.cable == "acsr-drake-26-7")
        .map(|s| &s.id)
        .collect();
    assert!(!on_drake.is_empty());
    assert!(on_drake.iter().all(|s| o.differs.sections.contains(*s)));
    let own = o.library.clone().expect("its own wire library");
    assert_ne!(own.get("acsr-drake-26-7"), m.library.get("acsr-drake-26-7"));
    // Solved with its own data.
    m.ensure_solved(0);
    let ctx = egui::Context::default();
    let start = std::time::Instant::now();
    while !m.other_lines[0].cases[0].ready {
        m.want_others(&[0], &ctx);
        m.poll_others();
        assert!(start.elapsed().as_secs() < 60, "not solved in a minute");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let o = &m.other_lines[0];
    let full = model::Solved::new(&o.line, &own, m.cases[0].case.clone());
    assert_eq!(o.cases[0].shapes, full.shapes);
}

/// What differs between two versions of a line, as the sections to solve
/// again: a setting every solve reads (the solver's, the defaults) makes
/// every section differ; a spacer changed, the sections it joins; a
/// jumper on a cable whose data differ, that jumper.
#[test]
fn the_differences_of_two_lines_say_what_to_solve_again() {
    use crate::model::Differences;
    let ours = synthetic().line;
    let same = Differences::between_with(&ours, &ours, &Default::default());
    assert!(same.sections.is_empty() && same.jumpers.is_empty());
    let mut other = ours.clone();
    other.defaults.jumper_exit_angle_deg += 10.0;
    let d = Differences::between_with(&ours, &other, &Default::default());
    assert_eq!(d.sections.len(), ours.sections.len());
    let mut other = ours.clone();
    other.solver.loads_per = match ours.solver.loads_per {
        ohl_line::LoadsPer::Stretched => ohl_line::LoadsPer::Unstretched,
        ohl_line::LoadsPer::Unstretched => ohl_line::LoadsPer::Stretched,
    };
    let d = Differences::between_with(&ours, &other, &Default::default());
    assert_eq!(d.sections.len(), ours.sections.len());
    if let Some(j) = ours.jumpers.first() {
        let cables: std::collections::BTreeSet<String> = [j.cable.clone()].into();
        let d = Differences::between_with(&ours, &ours, &cables);
        assert!(d.jumpers.contains(&j.id));
    }
}
