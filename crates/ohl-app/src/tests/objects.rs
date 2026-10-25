// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: objects added, edited and deleted with the tools and the tables,
//! libraries saved, alignment points.

use super::*;

/// Right-clicks `target` with `tool` held (as a view reports it).
pub(super) fn use_tool(
    h: &mut Harness<'_, App>,
    tool: crate::tools::Tool,
    target: Selection,
    at: Option<[f64; 3]>,
) {
    let r = h.state_mut().ready().unwrap();
    r.hotbar.hold(tool);
    r.use_tool(crate::selection::Use {
        target,
        at,
        on: None,
    });
    h.run();
}

pub(super) fn string_at(structure: &str, set: u32) -> Selection {
    Selection::Attachment {
        structure: structure.into(),
        set,
    }
}

/// Add a structure on the ground, a string on it, string a wire to it from
/// T5, change the section's tension and the string, then delete the
/// string with the Delete tool (the section goes with it, after the
/// confirmation): each step is one session command, solved at once.
#[test]
fn structures_strings_and_wires_are_added_edited_and_deleted() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project("tools");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let enter = |h: &mut Harness<'_, App>| press(h, egui::Key::Enter, egui::Modifiers::NONE);
    // Add structure: 150 m beyond T5, on the line's axis.
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
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Structure(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.id, "T8");
        f.structure_type = None;
        f.on_ground = false;
    }
    harness.get_by_label("Add structure");
    enter(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        assert_eq!(r.model.line.structure("T8").unwrap().position, at);
        assert_eq!(r.selection, Selection::Structure("T8".into()));
    }
    // Add string: T5's set 1 copied in its own axes (an I-string: its top,
    // its wire point, its weight).
    let t5_string =
        crate::editors::StringForm::of(&harness.state_mut().ready().unwrap().model, "T5", 1)
            .unwrap();
    let t5_top = t5_string
        .points
        .iter()
        .find(|p| p.label == t5_string.strings[0].top)
        .unwrap()
        .at;
    let t5_wire = t5_string.wires[0].at;
    use_tool(
        &mut harness,
        Tool::AddString,
        Selection::Structure("T8".into()),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::String(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.set, 1);
        let top = f.strings[0].top.clone();
        f.points.iter_mut().find(|p| p.label == top).unwrap().at = t5_top;
        f.wires[0].at = t5_wire;
        f.strings[0].weight = t5_string.strings[0].weight;
    }
    enter(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let a = r
            .model
            .line
            .wire_point(&ohl_line::SectionPoint::new("T8", 1))
            .unwrap();
        assert!((a.position[2] - (at[2] + t5_wire[2])).abs() < 1e-9);
    }
    // String wire: T5 1:1 → T8 1:1, Enter: the section window, like S1.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .select(Selection::Span {
            section: "S1".into(),
            span: 0,
        });
    use_tool(&mut harness, Tool::StringWire, string_at("T5", 1), None);
    use_tool(&mut harness, Tool::StringWire, string_at("T8", 1), None);
    assert_eq!(harness.state_mut().ready().unwrap().view3d.draft.len(), 2);
    enter(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Section(f)) = &r.editor else {
            panic!("{:?}", r.editor);
        };
        assert!(f.new);
        assert_eq!(f.id, "S4");
        assert_eq!(f.cable, "acsr-drake-26-7");
        assert!(r.chain.is_empty() && r.view3d.draft.is_empty());
    }
    enter(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        // (The cases not drawn are solved again when wanted.)
        for k in 0..r.model.cases.len() {
            r.model.ensure_solved(k);
        }
        for c in &r.model.cases {
            assert!(
                c.shapes.iter().any(|s| s.section == "S4"),
                "{}",
                c.case.name
            );
            assert!(c.failed.is_empty(), "{:?}", c.failed);
        }
        assert_solved_fully(&r.model);
    }
    // Edit the section: a higher tension, less sag.
    let sag = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.model.cases[0]
            .shapes
            .iter()
            .find(|s| s.section == "S4")
            .unwrap()
            .spans[0]
            .sag
    };
    let before = sag(&mut harness);
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::Span {
            section: "S4".into(),
            span: 0,
        },
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Section(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        let ohl_line::Stringing::Tension {
            horizontal_tension, ..
        } = &mut f.stringing
        else {
            panic!("{:?}", f.stringing);
        };
        *horizontal_tension *= 1.5;
    }
    enter(&mut harness);
    assert!(sag(&mut harness) < before * 0.8);
    // Edit the string: 1 m shorter.
    use_tool(&mut harness, Tool::Edit, string_at("T8", 1), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::String(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        f.wires[0].at[2] += 1.0;
    }
    enter(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        let a = r
            .model
            .line
            .wire_point(&ohl_line::SectionPoint::new("T8", 1))
            .unwrap();
        assert!((a.position[2] - (at[2] + t5_wire[2] + 1.0)).abs() < 1e-9);
        assert_solved_fully(&r.model);
    }
    // Delete the string: S4 goes with it, after the confirmation.
    use_tool(&mut harness, Tool::Delete, string_at("T8", 1), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let (impact, _) = r.confirm.as_ref().unwrap();
        assert_eq!(impact.sections, vec!["S4"]);
    }
    harness.get_by_label("Delete all").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.confirm.is_none());
        assert!(r.model.line.structure("T8").unwrap().wires.is_empty());
        assert!(r.model.line.sections.iter().all(|s| s.id != "S4"));
    }
    // Undo brings both back.
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.sections.iter().any(|s| s.id == "S4"));
    assert_solved_fully(&r.model);
}

/// A section's points picked again: one more structure in the middle;
/// unstressed lengths for the old spans become a tension.
#[test]
fn a_section_is_strung_again_through_other_points() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project("restring");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // S3 (T3 2:1 → T6 → T7) without T6.
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::Span {
            section: "S3".into(),
            span: 0,
        },
        None,
    );
    harness.get_by_label("Pick points again").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none());
        assert_eq!(r.restring.as_deref(), Some("S3"));
        assert_eq!(r.hotbar.tool(), Tool::StringWire);
    }
    use_tool(&mut harness, Tool::StringWire, string_at("T3", 2), None);
    use_tool(&mut harness, Tool::StringWire, string_at("T7", 1), None);
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Section(f)) = &r.editor else {
            panic!("{:?}", r.editor);
        };
        assert!(!f.new);
        assert_eq!(f.points.len(), 2);
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.editor.is_none(), "{:?}", r.status);
    let s3 = r.model.line.sections.iter().find(|s| s.id == "S3").unwrap();
    assert_eq!(s3.points.len(), 2);
    assert_solved_fully(&r.model);
}

/// A section sagged to a sag in the ruling span, clipped in (lengths of
/// that condition, which it remembers), then back in the blocks.
#[test]
fn a_section_is_sagged_clipped_in_and_back_in_the_blocks() {
    use crate::editors::Form;
    use crate::tools::Tool;
    use ohl_line::Stringing;
    let model = synthetic_project("clip");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let s2 = Selection::Span {
        section: "S2".into(),
        span: 0,
    };
    let sag = Stringing::Sag {
        sag: 7.5,
        temperature: 15.0,
        span: None,
    };
    let edit = |h: &mut Harness<'_, App>,
                change: &dyn Fn(&mut crate::editors::SectionForm, &model::Model)| {
        use_tool(h, Tool::Edit, s2.clone(), None);
        {
            let r = h.state_mut().ready().unwrap();
            let Some(Form::Section(mut f)) = r.editor.clone() else {
                panic!("{:?}", r.editor);
            };
            change(&mut f, &r.model);
            r.editor = Some(Form::Section(f));
        }
        press(h, egui::Key::Enter, egui::Modifiers::NONE);
        let r = h.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        r.model
            .line
            .sections
            .iter()
            .find(|s| s.id == "S2")
            .unwrap()
            .stringing
            .clone()
    };
    let st = edit(&mut harness, &|f, _| f.stringing = sag.clone());
    assert_eq!(st, sag);
    let st = edit(&mut harness, &|f, m| f.clip_in(m).unwrap());
    let Stringing::UnstressedLengths {
        lengths, sagged, ..
    } = &st
    else {
        panic!("{st:?}");
    };
    assert_eq!(lengths.len(), 4);
    assert_eq!(sagged.as_deref(), Some(&sag));
    // The same shape at stringing as in the blocks, solved fully.
    assert_solved_fully(&harness.state_mut().ready().unwrap().model);
    let st = edit(&mut harness, &|f, m| f.stringing = f.unclipped(m, 0));
    assert_eq!(st, sag);
}

/// Criteria › Weather cases opens its page in a floating window; a saved
/// change reaches the model (solved again in the new cases).
#[test]
fn a_saved_library_reaches_the_model() {
    use crate::library::{LibraryKind, PageAction};
    let mut model = synthetic_project("library");
    let dir = model.sources.file.parent().unwrap().to_path_buf();
    let weather = dir.join("cases.ohlwc");
    std::fs::write(
        &weather,
        "format = \"ohl-weather-cases\"\nversion = 1\n\n[[case]]\nname = \"cold\"\ntemperature = \"-10 °C\"\n",
    )
    .unwrap();
    model.sources.weather = Some(weather.clone());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Criteria").click();
    harness.run();
    harness.get_by_label("Weather cases library").click();
    harness.run();
    harness.run();
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .libraries
            .contains_key(&LibraryKind::WeatherCases)
    );
    harness.get_by_label("Save");
    // A second case written, as the page saves it.
    std::fs::write(
        &weather,
        "format = \"ohl-weather-cases\"\nversion = 1\n\n[[case]]\nname = \"cold\"\ntemperature = \"-10 °C\"\n\n[[case]]\nname = \"hot\"\ntemperature = \"60 °C\"\n",
    )
    .unwrap();
    let r = harness.state_mut().ready().unwrap();
    r.library_saved(LibraryKind::WeatherCases, PageAction::Saved(weather));
    assert_eq!(r.model.cases.len(), 2, "{:?}", r.status);
    assert_eq!(r.model.cases[1].case.name, "hot");
    assert_solved_fully(&r.model);
}

/// "Use for the model" with a browsed wire file: one without the line's
/// cables does not load — the model keeps its wires; a good one is used.
#[test]
fn a_browsed_library_is_used_for_the_model_only_when_it_loads() {
    use crate::library::{LibraryKind, PageAction};
    let model = synthetic_project("use-library");
    let dir = model.sources.file.parent().unwrap().to_path_buf();
    let before = model.sources.library.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let theirs = dir.join("theirs.ohlw");
    std::fs::write(&theirs, LibraryKind::Cables.template("theirs")).unwrap();
    r.library_saved(LibraryKind::Cables, PageAction::UseForModel(theirs));
    assert!(r.status.as_ref().unwrap().1, "{:?}", r.status);
    assert_eq!(r.model.sources.library, before, "kept");
    let good = dir.join("good.ohlw");
    std::fs::copy(root().join("libraries/cables/acsr-examples.ohlw"), &good).unwrap();
    r.library_saved(LibraryKind::Cables, PageAction::UseForModel(good.clone()));
    assert!(!r.status.as_ref().unwrap().1, "{:?}", r.status);
    assert_eq!(r.model.sources.library, Some(good));
    assert_solved_fully(&r.model);
}

/// Sets a cell of an existing row of `kind`.
pub(super) fn set_cell(
    h: &mut Harness<'_, App>,
    kind: crate::tables::TableKind,
    key: &str,
    col: &str,
    text: &str,
) {
    let c = kind.column(col);
    let r = h.state_mut().ready().unwrap();
    let rows = kind.rows(&r.model, r.case);
    let original = rows.iter().find(|x| x.key == key).unwrap().cells[c].clone();
    r.tables
        .entry(kind)
        .or_default()
        .set(key, c, text, &original);
}

/// Apply (deletions confirmed when `confirm`); the error if it failed.
pub(super) fn apply_table(
    h: &mut Harness<'_, App>,
    kind: crate::tables::TableKind,
    confirm: bool,
) -> Result<(), String> {
    let r = h.state_mut().ready().unwrap();
    let mut page = r.tables.remove(&kind).unwrap_or_default();
    let prepared = page.prepare(kind, &r.model, r.case, confirm);
    r.tables.insert(kind, page);
    match prepared? {
        Some(c) => {
            r.apply_table(kind, c);
            match &r.tables[&kind].message {
                Some((m, true)) => Err(m.clone()),
                _ => Ok(()),
            }
        }
        None => Err("needs a confirmation".into()),
    }
}

/// Every table edits the line in one command (one undo), renames keep what
/// refers to them, a cell that does not read blocks Apply.
#[test]
fn tables_edit_the_line_in_one_command_each() {
    use crate::tables::TableKind;
    let model = synthetic_project("tables");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // Opened from the menu.
    // The menu button (the Model page has a "Structures" layer too).
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Structures")
        .click();
    harness.run();
    harness.get_by_label("Structures table").click();
    harness.run();
    harness.run();
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .tables
            .contains_key(&TableKind::Structures)
    );
    let (t2, t4) = {
        let r = harness.state_mut().ready().unwrap();
        (
            r.model.line.structure("T2").unwrap().position,
            r.model.line.structure("T4").unwrap().position,
        )
    };
    set_cell(
        &mut harness,
        TableKind::Structures,
        "T2",
        "E",
        &format!("{}", t2[0] + 2.0),
    );
    set_cell(
        &mut harness,
        TableKind::Structures,
        "T4",
        "z",
        &format!("{}", t4[2] + 1.0),
    );
    harness.run();
    harness.get_by_label("Apply 2 changes");
    set_cell(&mut harness, TableKind::Structures, "T4", "N", "north");
    let err = apply_table(&mut harness, TableKind::Structures, false).unwrap_err();
    assert!(err.contains("not a number"), "{err}");
    set_cell(
        &mut harness,
        TableKind::Structures,
        "T4",
        "N",
        &format!("{:.3}", t4[1]),
    );
    apply_table(&mut harness, TableKind::Structures, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.tables[&TableKind::Structures].pending(), 0);
        assert!((r.model.line.structure("T2").unwrap().position[0] - t2[0] - 2.0).abs() < 1e-9);
        assert!((r.model.line.structure("T4").unwrap().position[2] - t4[2] - 1.0).abs() < 1e-9);
        assert_solved_fully(&r.model);
    }
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.line.structure("T2").unwrap().position, t2);
        assert_eq!(
            r.model.line.structure("T4").unwrap().position,
            t4,
            "one undo"
        );
    }
    // Renamed and moved at once: the sections follow.
    set_cell(&mut harness, TableKind::Structures, "T3", "id", "Y3");
    set_cell(
        &mut harness,
        TableKind::Structures,
        "T3",
        "orientation °",
        "5",
    );
    apply_table(&mut harness, TableKind::Structures, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.line.structures[2].id, "Y3");
        assert_eq!(r.model.line.structures[2].orientation_deg, 5.0);
        assert!(
            r.model.line.sections[0]
                .points
                .iter()
                .any(|p| p.structure == "Y3")
        );
        assert_solved_fully(&r.model);
    }
    // Sections: condition by sag, with its value; a string's weight; a
    // jumper's sag; a system's voltage and phases.
    set_cell(
        &mut harness,
        TableKind::Sections,
        "S1",
        "condition",
        "sag, span 2",
    );
    set_cell(&mut harness, TableKind::Sections, "S1", "value", "8");
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    assert_eq!(
        harness.state_mut().ready().unwrap().model.line.sections[0].stringing,
        ohl_line::Stringing::Sag {
            sag: 8.0,
            temperature: 15.0,
            span: Some(1)
        }
    );
    set_cell(
        &mut harness,
        TableKind::Strings,
        "Y3\tS2",
        "weight N",
        "1234",
    );
    apply_table(&mut harness, TableKind::Strings, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let a = r.model.line.structure("Y3").unwrap().string("S2").unwrap();
        assert_eq!(a.weight, 1234.0);
        assert_eq!(r.model.line.sections[2].points[0].set, 2);
    }
    set_cell(
        &mut harness,
        TableKind::Jumpers,
        "J-T3-branch",
        "sag m",
        "2",
    );
    apply_table(&mut harness, TableKind::Jumpers, false).unwrap();
    assert_eq!(
        harness.state_mut().ready().unwrap().model.line.jumpers[0].sag,
        Some(2.0)
    );
    let id = harness.state_mut().ready().unwrap().model.line.systems[0]
        .id
        .clone();
    set_cell(&mut harness, TableKind::Systems, &id, "kV", "230");
    set_cell(&mut harness, TableKind::Systems, &id, "id", "sys-230");
    apply_table(&mut harness, TableKind::Systems, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.line.systems[0].voltage_kv, 230.0, "{:?}", r.status);
    assert_eq!(r.model.line.systems[0].id, "sys-230");
    assert!(
        r.model
            .line
            .sections
            .iter()
            .any(|s| s.system.as_deref() == Some("sys-230"))
    );
    assert_solved_fully(&r.model);
}

/// Rows added and deleted; a deletion taking sections waits for its
/// confirmation.
#[test]
fn table_rows_are_added_and_deleted() {
    use crate::tables::TableKind;
    let model = synthetic_project("table-rows");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let add = |h: &mut Harness<'_, App>, kind: TableKind, cells: &[(&str, &str)]| {
        let r = h.state_mut().ready().unwrap();
        let mut row = kind.new_row(&r.model, r.case, &[]);
        for (col, text) in cells {
            row[kind.column(col)] = (*text).to_owned();
        }
        r.tables.entry(kind).or_default().added.push(row);
    };
    let t5 = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .structure("T5")
        .unwrap()
        .position;
    add(
        &mut harness,
        TableKind::Structures,
        &[
            ("id", "T8"),
            ("E", &format!("{}", t5[0] + 200.0)),
            ("N", &format!("{}", t5[1])),
            ("z", &format!("{}", t5[2])),
        ],
    );
    apply_table(&mut harness, TableKind::Structures, false).unwrap();
    // A string on it (the strings table changes strings; points and wire
    // points come with the string editor).
    use_tool(
        &mut harness,
        crate::tools::Tool::AddString,
        Selection::Structure("T8".into()),
        None,
    );
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    add(
        &mut harness,
        TableKind::Sections,
        &[("id", "S4"), ("points", "T5 1:1, T8 1:1"), ("value", "20")],
    );
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(
            r.model.line.sections.iter().any(|s| s.id == "S4"),
            "{:?}",
            r.status
        );
        assert_solved_fully(&r.model);
    }
    // Deleting T8 takes S4: listed first, then applied.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .tables
        .get_mut(&TableKind::Structures)
        .unwrap()
        .deleted
        .insert("T8".into());
    assert!(apply_table(&mut harness, TableKind::Structures, false).is_err());
    {
        let r = harness.state_mut().ready().unwrap();
        let takes = r.tables[&TableKind::Structures].confirm.clone().unwrap();
        assert!(takes.iter().any(|t| t.contains("S4")), "{takes:?}");
        assert!(r.model.line.structure("T8").is_some(), "not yet");
    }
    apply_table(&mut harness, TableKind::Structures, true).unwrap();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.structure("T8").is_none());
    assert!(r.model.line.sections.iter().all(|s| s.id != "S4"));
}

/// A target largest sag in the section's display weather case: the
/// condition giving it is found; the display case is per section.
#[test]
fn sections_get_a_target_sag_in_their_display_case() {
    use crate::tables::TableKind;
    let model = synthetic_project("table-target");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let hot = {
        let r = harness.state_mut().ready().unwrap();
        r.model
            .cases
            .iter()
            .position(|c| c.case.name == "max temperature")
            .unwrap()
    };
    // Two display cases: drawn in both, the first reported.
    set_cell(
        &mut harness,
        TableKind::Sections,
        "S1",
        "display cases",
        "max temperature; glaze ice",
    );
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let ice = r
            .model
            .cases
            .iter()
            .position(|c| c.case.name == "glaze ice")
            .unwrap();
        assert_eq!(r.model.section_cases("S1", 0), vec![hot, ice]);
        assert_eq!(r.model.section_case("S1", 0), hot);
        assert_eq!(r.model.section_cases("S3", 0), vec![0]);
        let shown = r.model.shapes_shown(0);
        assert_eq!(shown.iter().filter(|x| x.section == "S1").count(), 2);
        let rows = TableKind::Sections.rows(&r.model, 0);
        assert!(rows[0].cells[TableKind::Sections.column("max sag m")].contains(" / "));
    }
    set_cell(&mut harness, TableKind::Sections, "S1", "max sag m", "12");
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let sag = r
        .model
        .shape("S1", 0)
        .unwrap()
        .spans
        .iter()
        .map(|s| s.sag)
        .fold(0.0, f64::max);
    assert!((sag - 12.0).abs() < 0.01, "{sag}");
    assert_eq!(r.model.shape("S1", 0).unwrap().section, "S1");
    let rows = TableKind::Sections.rows(&r.model, 0);
    assert_eq!(
        rows[0].cells[TableKind::Sections.column("display cases")],
        "max temperature; glaze ice"
    );
}

/// Alignment points: moved, detached, added between two, a row deleted —
/// one command; a row deleted takes its point off that alignment only (a
/// shared point stays on the other), the point going with its last row.
#[test]
fn alignment_points_are_edited_in_their_table() {
    use crate::tables::TableKind;
    let model = synthetic_project("table-alignment");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (n, p0, key0, key1) = {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::AlignmentPoints.rows(&r.model, 0);
        let mine: Vec<&crate::tables::Row> = rows.iter().filter(|x| x.cells[0] == "main").collect();
        (
            mine.len(),
            mine[0].cells.clone(),
            mine[0].key.clone(),
            mine[1].key.clone(),
        )
    };
    assert!(n >= 2, "{n}");
    let e = p0[3].parse::<f64>().unwrap() + 3.0;
    set_cell(
        &mut harness,
        TableKind::AlignmentPoints,
        &key0,
        "E",
        &format!("{e}"),
    );
    set_cell(
        &mut harness,
        TableKind::AlignmentPoints,
        &key0,
        "structure",
        "",
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let mut row = TableKind::AlignmentPoints.new_row(&r.model, 0, &[]);
        row[0] = "main".into();
        row[1] = "1.5".into();
        row[3] = format!("{e}");
        row[4] = p0[4].clone();
        let page = r.tables.entry(TableKind::AlignmentPoints).or_default();
        page.added.push(row);
        page.deleted.insert(key1.clone());
    }
    // The point's row on main deleted: off main, still on the branch
    // (nothing to confirm).
    let p1 = key1.split_once('\t').unwrap().1.to_owned();
    let shared = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .point_alignments(&p1)
        .len();
    apply_table(&mut harness, TableKind::AlignmentPoints, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let line = &r.model.line;
        assert!(line.alignment_point(&p1).is_some(), "the point stays");
        let on = line.point_alignments(&p1);
        assert!(!on.contains(&"main".to_owned()), "{on:?}");
        assert_eq!(on.len(), shared - 1);
    }
    let r = harness.state_mut().ready().unwrap();
    let rows = TableKind::AlignmentPoints.rows(&r.model, 0);
    let mine: Vec<&crate::tables::Row> = rows.iter().filter(|x| x.cells[0] == "main").collect();
    assert_eq!(mine.len(), n, "one added, one deleted");
    assert_eq!(mine[0].cells[6], "", "detached");
    assert!((mine[0].cells[3].parse::<f64>().unwrap() - e).abs() < 1e-6);
    assert!(
        (mine[1].cells[3].parse::<f64>().unwrap() - e).abs() < 1e-6,
        "the new one second"
    );
    // Its last rows deleted: the point goes.
    let keys: Vec<String> = rows
        .iter()
        .filter(|x| x.key.split_once('\t').is_some_and(|(_, p)| p == p1))
        .map(|x| x.key.clone())
        .collect();
    assert!(!keys.is_empty());
    r.tables
        .entry(TableKind::AlignmentPoints)
        .or_default()
        .deleted
        .extend(keys);
    apply_table(&mut harness, TableKind::AlignmentPoints, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.alignment_point(&p1).is_none());
}

/// Weather cases: changed, added (between two), deleted — written to the
/// case file (comments kept), the model solved again.
#[test]
fn the_weather_case_table_writes_its_file() {
    use crate::tables::TableKind;
    let mut model = synthetic_project("weather-table");
    let path = model.sources.file.parent().unwrap().join("cases.ohlwc");
    // With a group of two of its cases (2026-10-25): kept by the table,
    // its cases renamed with them.
    let text = std::fs::read_to_string(root().join("examples/synthetic-line/weather-cases.ohlwc"))
        .unwrap()
        + "\n[[group]]\nname = \"ice\"\ndescription = \"both ices\"\ncases = [\"glaze ice\", \"rime ice + wind\"]\n";
    std::fs::write(&path, text).unwrap();
    model.sources.weather = Some(path.clone());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let kind = TableKind::WeatherCases;
    let n = harness.state_mut().ready().unwrap().model.cases.len();
    {
        // The air density factor's default shown, dimmed.
        let r = harness.state_mut().ready().unwrap();
        let rows = kind.rows(&r.model, 0);
        let q = kind.column("Air density factor\nPa/(m/s)²");
        assert_eq!(rows[0].cells[q], "0.613");
        assert!(rows[0].defaulted[q]);
    }
    set_cell(&mut harness, kind, "2", "Ice thickness\nmm", "20");
    set_cell(
        &mut harness,
        kind,
        "2",
        "Tags\n ",
        "clearance, loads, heavy",
    );
    set_cell(&mut harness, kind, "2", "Ambient temp\n°C", "-5");
    // A velocity typed: the pressure is Q·V² (Q 0.613, then 0.6).
    set_cell(&mut harness, kind, "1", "Wind velocity\nm/s", "40");
    set_cell(
        &mut harness,
        kind,
        "3",
        "Air density factor\nPa/(m/s)²",
        "0.6",
    );
    set_cell(&mut harness, kind, "3", "Wind velocity\nm/s", "20");
    set_cell(&mut harness, kind, "3", "Name\n ", "rime ice and wind");
    set_cell(&mut harness, kind, "0", "Wire temp\n°C", "warm");
    assert!(apply_table(&mut harness, kind, false).is_err());
    set_cell(&mut harness, kind, "0", "Wire temp\n°C", "10.0");
    {
        let r = harness.state_mut().ready().unwrap();
        let mut row = kind.new_row(&r.model, 0, &[]);
        row[kind.column("#\n ")] = "1.5".into();
        row[kind.column("Name\n ")] = "cold".into();
        row[kind.column("Wire temp\n°C")] = "-20".into();
        let page = r.tables.entry(kind).or_default();
        page.added.push(row);
        page.deleted.insert("4".into());
    }
    apply_table(&mut harness, kind, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let names: Vec<&str> = r.model.cases.iter().map(|c| c.case.name.as_str()).collect();
    assert_eq!(names.len(), n, "{names:?}");
    assert_eq!(names[1], "cold");
    let glaze = r
        .model
        .cases
        .iter()
        .find(|c| c.case.name == "glaze ice")
        .unwrap();
    assert!((glaze.case.ice_thickness.si() - 0.02).abs() < 1e-12);
    assert_eq!(glaze.case.tags, vec!["clearance", "loads", "heavy"]);
    assert_eq!(
        glaze.case.ambient_temperature.map(|t| t.celsius()),
        Some(-5.0)
    );
    let case = |name: &str| {
        &r.model
            .cases
            .iter()
            .find(|c| c.case.name == name)
            .unwrap()
            .case
    };
    let high = case("max temperature");
    assert!(
        (high.wind_pressure.si() - 0.613 * 1600.0).abs() < 1e-6,
        "{}",
        high.wind_pressure.si()
    );
    assert!((high.wind_velocity() - 40.0).abs() < 1e-9);
    let rime = case("rime ice and wind");
    assert_eq!(rime.air_density_factor, Some(0.6));
    assert!(
        (rime.wind_pressure.si() - 240.0).abs() < 1e-6,
        "{}",
        rime.wind_pressure.si()
    );
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        written.starts_with("# Example weather cases"),
        "comments kept"
    );
    let groups = ohl_weather::WeatherCases::parse(&written).unwrap().groups;
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].cases, vec!["glaze ice", "rime ice and wind"]);
    assert_solved_fully(&r.model);
}

/// Weather case groups on structures and sections (owner, 2026-10-25),
/// typed in their tables as a comma list: a group the case file does not
/// have is refused (naming those it has); kept on the object, nothing
/// solved again for them.
#[test]
fn structures_and_sections_take_weather_groups() {
    use crate::tables::TableKind;
    let mut model = synthetic_project("structure-groups");
    let path = model.sources.file.parent().unwrap().join("cases.ohlwc");
    let text = std::fs::read_to_string(root().join("examples/synthetic-line/weather-cases.ohlwc"))
        .unwrap()
        + "\n[[group]]\nname = \"internal\"\ncases = [\"glaze ice\"]\n\
           [[group]]\nname = \"snow\"\ncases = [\"everyday\"]\n";
    std::fs::write(&path, text).unwrap();
    model.sources.weather = Some(path);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let kind = TableKind::Structures;
    set_cell(
        &mut harness,
        kind,
        "T2",
        "weather groups",
        "internal; other",
    );
    let e = apply_table(&mut harness, kind, false).unwrap_err();
    assert!(e.contains("internal, snow"), "{e}");
    set_cell(&mut harness, kind, "T2", "weather groups", "internal; snow");
    apply_table(&mut harness, kind, false).unwrap();
    let s1 = harness.state_mut().ready().unwrap().model.line.sections[0]
        .id
        .clone();
    set_cell(
        &mut harness,
        TableKind::Sections,
        &s1,
        "weather groups",
        "snow",
    );
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let t2 = r.model.line.structure("T2").unwrap();
    assert_eq!(t2.weather_groups, vec!["internal", "snow"]);
    assert_eq!(r.model.line.sections[0].weather_groups, vec!["snow"]);
    // The overrides' filter: at T2 the glaze ice and everyday cases only,
    // on the section everyday only; elsewhere every case.
    let f = crate::case_groups::Overrides {
        structures: true,
        sections: true,
    }
    .filter(&r.model);
    assert!(f.takes(Some("T2"), None, "glaze ice"));
    assert!(!f.takes(Some("T2"), None, "max temperature"));
    assert!(!f.takes(None, Some(&s1), "glaze ice"));
    assert!(f.takes(Some("T3"), None, "max temperature"));
}

/// Alignment points with the tools: added on the ground (into the nearest
/// segment; a route following its structures gets points under them
/// first), moved in the Edit window, deleted.
#[test]
fn alignment_points_are_added_edited_and_deleted_with_the_tools() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project("alignment-tools");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (t1, t2) = {
        let r = harness.state_mut().ready().unwrap();
        let l = &r.model.line;
        (
            l.structure("T1").unwrap().position,
            l.structure("T2").unwrap().position,
        )
    };
    // Between T1 and T2, 5 m aside: nothing to snap to, the coordinates
    // given in its window.
    let at = [0.5 * (t1[0] + t2[0]), 0.5 * (t1[1] + t2[1]) + 5.0, t1[2]];
    use_tool(&mut harness, Tool::AddAlignment, Selection::None, Some(at));
    {
        let r = harness.state_mut().ready().unwrap();
        let d = r.coord_dialog.as_mut().expect("asked");
        assert_eq!(d.purpose, crate::alignment_tools::CoordFor::AddAlignment);
        assert!(!d.connect, "not connected unless ticked");
        d.text = at.map(|v| format!("{v}"));
        d.connect = true;
    }
    harness.run();
    harness.get_by_label("Add the point").click();
    harness.run();
    let id = {
        let r = harness.state_mut().ready().unwrap();
        let Selection::AlignmentPoint { id } = r.selection.clone() else {
            panic!("{:?} {:?}", r.selection, r.status);
        };
        id
    };
    let route = "main".to_owned();
    let points = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.model.line.alignment_by_id("main").unwrap().points.clone()
    };
    let before = points(&mut harness);
    assert_eq!(before[1], id, "between the first two");
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.line.alignment_point(&id).unwrap().position, at);
    }
    // Moved 2 m east in its window.
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::AlignmentPoint { id: id.clone() },
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Alignment(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.orders, [(route.clone(), 2)]);
        f.position[0] += 2.0;
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(
            r.model.line.alignment_point(&id).unwrap().position[0],
            at[0] + 2.0
        );
    }
    // Deleted: off the alignment, the others in order.
    use_tool(
        &mut harness,
        Tool::Delete,
        Selection::AlignmentPoint { id: id.clone() },
        None,
    );
    let mut kept = before.clone();
    kept.remove(1);
    assert_eq!(points(&mut harness), kept);
    // A shared point (on the branch too): the Delete tool asks which
    // alignments; off the branch only, it stays on this one.
    let shared = points(&mut harness).into_iter().find(|p| {
        harness
            .state_mut()
            .ready()
            .unwrap()
            .model
            .line
            .point_alignments(p)
            .len()
            > 1
    });
    {
        let p = shared.expect("a point on the branch too");
        use_tool(
            &mut harness,
            Tool::Delete,
            Selection::AlignmentPoint { id: p.clone() },
            None,
        );
        {
            let r = harness.state_mut().ready().unwrap();
            let (id, ticks) = r.point_delete.as_mut().expect("asked which alignments");
            assert_eq!(*id, p);
            // Only the other alignments ticked.
            for (a, on) in ticks.iter_mut() {
                *on = *a != route;
            }
        }
        harness.run();
        harness.get_by_label("Take it off the ticked ones").click();
        harness.run();
        let r = harness.state_mut().ready().unwrap();
        assert!(r.point_delete.is_none());
        assert_eq!(r.model.line.point_alignments(&p), vec![route.clone()]);
    }
    // Down to none: the alignment stays (a shared point: "Delete from
    // all", then the confirmation of what goes with it).
    while let Some(first) = points(&mut harness).first().cloned() {
        use_tool(
            &mut harness,
            Tool::Delete,
            Selection::AlignmentPoint { id: first.clone() },
            None,
        );
        if harness.state_mut().ready().unwrap().point_delete.is_some() {
            harness.get_by_label("Delete from all").click();
            harness.run();
        }
        let asked = harness.state_mut().ready().unwrap().confirm.is_some();
        if asked {
            harness.get_by_label("Delete all").click();
            harness.run();
        }
        let r = harness.state_mut().ready().unwrap();
        assert!(r.model.line.alignment_point(&first).is_none());
    }
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.alignment_by_id(&route).is_some());
}
