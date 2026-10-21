// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The section library typed in the window: new, edit, duplicate, delete;
//! the checks; undo and redo in order with the structure file's edits; a
//! member taking the new section. Every dimension here is invented.

use egui_kittest::kittest::Queryable;
use ohl_structure_edit::section_library::SectionDraft;

use crate::app::{Sheet, StructureApp};
use crate::layout::Panel;
use crate::section_editor::{Editing, NEW_FILE, section_choices};
use crate::tests::{copy_of, harness, menu};

/// A library file with a material, beside a copy of type-02.
const MATERIALS: &str = "format = \"ohl-sections\"\nversion = 1\ndefault_material = \"st\"\n\
    [[material]]\nid = \"st\"\nelasticity = \"210 GPa\"\ndensity = \"7850 kg/m3\"\n";

fn app(tag: &str, with_file: bool) -> (StructureApp, std::path::PathBuf) {
    let path = copy_of("type-02.ohls", tag);
    let sections = path.parent().unwrap().join("Sections");
    let _ = std::fs::remove_dir_all(&sections);
    if with_file {
        std::fs::create_dir_all(&sections).unwrap();
        std::fs::write(sections.join("a.ohlsec"), MATERIALS).unwrap();
    }
    let mut app = StructureApp::new(None);
    app.open(&path);
    (app, sections)
}

fn angle(file: &std::path::Path, id: &str, h: &str, t: &str) -> Editing {
    let mut draft = SectionDraft::new(id, "equal_angle");
    draft.dims.insert("h".into(), h.into());
    draft.dims.insert("t".into(), t.into());
    Editing {
        file: file.to_path_buf(),
        old: None,
        draft,
    }
}

#[test]
fn a_typed_section_is_saved_used_and_undone_in_order() {
    let (mut app, sections) = app("seclib-order", true);
    let file = sections.join("a.ohlsec");
    assert_eq!(app.section_target().unwrap(), file);
    // Equal angle 60 × 6: A = 684 mm², 684e-6 × 7850 = 5.369 kg/m.
    let ed = angle(&file, "L60", "60 mm", "6 mm");
    let check = app.check_draft(&ed);
    let s = check.section.as_ref().unwrap();
    assert!((s.properties().area - 684e-6).abs() < 1e-12);
    assert_eq!(check.density, Some(7850.0));
    assert!(check.warnings.is_empty(), "{:?}", check.warnings);
    app.save_section(&ed).unwrap();
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("id = \"L60\"")
    );
    // Read again: the members may take it.
    assert!(app.loads.sections.section("L60").is_some());
    let doc = app.file.as_ref().unwrap().doc().clone();
    assert!(section_choices(&app.loads.sections, &doc).contains(&"L60".to_owned()));
    // A member takes it: an edit of the structure file, after the
    // library's.
    app.file.as_mut().unwrap().edit("section of m1", |doc, _| {
        ohl_structure_edit::set_member_section(doc, "m1", Some("L60"))
    });
    assert_eq!(app.undo_name().as_deref(), Some("section of m1"));
    app.undo();
    assert_eq!(
        app.undo_name().as_deref(),
        Some("cross-section library: new cross-section L60")
    );
    app.undo();
    assert!(!std::fs::read_to_string(&file).unwrap().contains("L60"));
    assert!(app.loads.sections.section("L60").is_none());
    // Redo: the library first, then the member.
    assert_eq!(
        app.redo_name().as_deref(),
        Some("cross-section library: new cross-section L60")
    );
    app.redo();
    assert!(std::fs::read_to_string(&file).unwrap().contains("L60"));
    assert_eq!(app.redo_name().as_deref(), Some("section of m1"));
    app.redo();
    assert!(
        app.file
            .as_ref()
            .unwrap()
            .text()
            .contains("section = \"L60\"")
    );
    std::fs::remove_dir_all(sections.parent().unwrap()).unwrap();
}

#[test]
fn edits_duplicates_and_deletions_are_checked() {
    let (mut app, sections) = app("seclib-edit", true);
    let file = sections.join("a.ohlsec");
    app.save_section(&angle(&file, "L60", "60 mm", "6 mm"))
        .unwrap();
    // A second file: an id there is taken across the library.
    let other = sections.join("b.ohlsec");
    std::fs::write(
        &other,
        "[[section]]\nid = \"B\"\nshape = \"round_bar\"\nd = \"20 mm\"\n",
    )
    .unwrap();
    app.section_editor.forget_files();
    let e = app
        .check_draft(&angle(&file, "B", "60 mm", "6 mm"))
        .section
        .unwrap_err();
    assert!(e.contains("taken") && e.contains("b.ohlsec"), "{e}");
    assert!(
        app.save_section(&angle(&file, "B", "60 mm", "6 mm"))
            .is_err()
    );
    // What is wrong with the dimensions, in the library's words.
    let e = app
        .check_draft(&angle(&file, "X", "60 mm", "61 mm"))
        .section
        .unwrap_err();
    assert!(e.contains("must not exceed a leg"), "{e}");
    // A material the library lacks: a warning, the section still reads.
    let mut ed = angle(&file, "X", "60 mm", "6 mm");
    ed.draft.material = "alloy".into();
    let c = app.check_draft(&ed);
    assert!(
        c.section.is_ok() && c.warnings[0].contains("alloy"),
        "{:?}",
        c.warnings
    );
    // Duplicate: a free id, the same dimensions; then edit it in place.
    let copy = app.duplicate_section(&file, "L60").unwrap();
    assert_eq!(copy.draft.id, "L60-copy");
    assert_eq!(copy.draft.dims["t"], "6 mm");
    app.save_section(&copy).unwrap();
    let mut thicker = copy.clone();
    thicker.old = Some("L60-copy".into());
    thicker.draft.id = "L60x8".into();
    thicker.draft.dims.insert("t".into(), "8 mm".into());
    app.save_section(&thicker).unwrap();
    let lib = ohl_sections::SectionLibrary::load(&sections).unwrap();
    let ids: Vec<&str> = lib.sections.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["L60", "L60x8", "B"]);
    // Delete, undone.
    app.delete_section(&file, "L60").unwrap();
    assert!(app.loads.sections.section("L60").is_none());
    app.undo();
    assert!(app.loads.sections.section("L60").is_some());
    // A file changed outside since: the undo says so and leaves it.
    std::fs::write(&file, MATERIALS).unwrap();
    app.undo();
    assert!(app.message.as_ref().unwrap().1, "{:?}", app.message);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), MATERIALS);
    std::fs::remove_dir_all(sections.parent().unwrap()).unwrap();
}

#[test]
fn the_first_section_makes_the_library_file() {
    let (mut app, sections) = app("seclib-new", false);
    let target = app.section_target().unwrap();
    assert_eq!(target, sections.join(NEW_FILE));
    let ed = angle(&target, "L50", "50 mm", "5 mm");
    let c = app.check_draft(&ed);
    assert!(c.warnings[0].contains("no material"), "{:?}", c.warnings);
    app.save_section(&ed).unwrap();
    assert!(target.is_file());
    // Undone: the file it made goes again.
    app.undo();
    assert!(!target.exists());
    std::fs::remove_dir_all(sections.parent().unwrap()).unwrap();
}

/// The sheet in the window: Design › Sections & materials, Section
/// library, + New section opens the form.
#[test]
fn the_section_library_sheet_opens_a_form() {
    let path = copy_of("type-02.ohls", "seclib-ui");
    let sections = path.parent().unwrap().join("Sections");
    std::fs::create_dir_all(&sections).unwrap();
    std::fs::write(sections.join("a.ohlsec"), MATERIALS).unwrap();
    let mut h = harness(&path);
    menu(&mut h, "Design", "Cross-sections & materials…");
    h.state_mut()
        .sheets
        .insert(Panel::SectionsMaterials, Sheet::Library);
    h.run();
    h.get_by_label("+ New cross-section").click();
    h.run();
    assert!(h.state().section_editor.editing.is_some());
    // Nothing typed yet: what is wrong is said; typed, the properties.
    h.get_by_value(ohl_structure_edit::section_library::NO_ID);
    let file = sections.join("a.ohlsec");
    h.state_mut().section_editor.editing = Some(angle(&file, "L60", "60 mm", "6 mm"));
    h.run();
    h.get_by_label("Properties (computed as typed)");
    h.get_by_value("5.369 kg/m");
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
