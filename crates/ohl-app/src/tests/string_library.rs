// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: Line › Structures › Put the project's strings in a library…
//! (owner, 2026-11-18): the preview, Escape, Apply as one command, the
//! results unchanged.

use super::*;

/// Every case solved (each its shapes), the line as it is.
fn solved_all_cases(m: &mut model::Model) -> Vec<Vec<ohl_line::solve::SectionShape>> {
    for k in 0..m.cases.len() {
        m.ensure_solved(k);
    }
    m.cases.iter().map(|c| c.shapes.clone()).collect()
}

/// The typed example (its structure types' strings written inline, its
/// structures' own strings too): the preview lists the entries to make
/// and the strings left inline with their reasons; Escape changes
/// nothing; Apply is one command — every grouped string names its entry
/// (in the structures and the type files), the entries in the library
/// with the strings' values exactly — and every weather case solves to
/// the same shapes, to the last digit; one undo takes it back.
#[test]
fn the_projects_strings_go_in_a_library_as_one_command() {
    let mut model = typed_project("string-library");
    let before = solved_all_cases(&mut model);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    let plan = crate::string_entries::plan(&h.state_mut().ready().unwrap().model).unwrap();
    assert!(!plan.groups.is_empty(), "{plan:?}");
    assert!(
        plan.strings() > plan.groups.len(),
        "strings share entries: {plan:?}"
    );
    // Escape: nothing changes.
    h.state_mut().ready().unwrap().open_string_library();
    h.run();
    h.get_by_label_contains("Entries to make");
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.state_mut().ready().unwrap().string_library.is_none());
    let entries = h
        .state_mut()
        .ready()
        .unwrap()
        .model
        .session()
        .unwrap()
        .log()
        .unwrap()
        .len();
    // Apply.
    h.state_mut().ready().unwrap().open_string_library();
    h.run();
    h.get_by_label("Apply").click();
    h.run();
    let r = h.state_mut().ready().unwrap();
    assert!(r.string_library.is_none(), "{:?}", r.string_library);
    assert_eq!(
        r.model.session().unwrap().log().unwrap().len(),
        entries + 1,
        "one command: {:?}",
        r.status
    );
    // Every string grouped names its entry; the entry holds its values.
    let library = r.model.types.as_ref().unwrap().1.clone();
    for g in &plan.groups {
        let a = library
            .get(&g.id)
            .unwrap_or_else(|| panic!("entry {}", g.id));
        let v = crate::string_entries::Values::of_entry(a, g.values.legs).unwrap();
        assert!(v.same(&g.values), "{}: {v:?} {:?}", g.id, g.values);
    }
    let stored = r.model.stored_line().unwrap();
    for g in &plan.groups {
        for (p, l) in &g.strings {
            if let crate::string_entries::Place::Structure(id) = p {
                let x = stored.structure(id).unwrap().string(l).unwrap();
                assert_eq!(x.insulator.as_deref(), Some(g.id.as_str()), "{id} {l}");
            }
        }
    }
    let types = r
        .model
        .session()
        .unwrap()
        .library_files("structures")
        .unwrap();
    for g in &plan.groups {
        for (p, l) in &g.strings {
            if let crate::string_entries::Place::Type(f) = p {
                let text = &types.iter().find(|(n, _)| n == f).unwrap().1;
                use ohl_structures::LibraryItem;
                let t =
                    ohl_structures::StructureType::parse(std::path::Path::new(f), text).unwrap();
                let x = t.strings.iter().find(|x| &x.label == l).unwrap();
                assert_eq!(x.insulator.as_deref(), Some(g.id.as_str()), "{f} {l}");
            }
        }
    }
    // The results: the same, every case.
    let after = solved_all_cases(&mut r.model);
    assert_eq!(after.len(), before.len());
    for (k, (a, b)) in after.iter().zip(&before).enumerate() {
        assert_eq!(a, b, "weather case {k}: the results changed");
    }
    // Nothing left to put: the plan is empty but for those left inline.
    let again = crate::string_entries::plan(&r.model).unwrap();
    assert!(again.groups.is_empty(), "{again:?}");
    // One undo takes it all back.
    r.model.undo().unwrap();
    let back = crate::string_entries::plan(&r.model).unwrap();
    assert_eq!(back.strings(), plan.strings());
}
