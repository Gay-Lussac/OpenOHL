// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Analysis row's switch of what the tension-only members carry in
//! compression (Design › Member cross-sections, owner 2026-11-15):
//! ticking "tension-only: no compression" writes `[analysis]
//! tension_compression = "none"` and raises the file to version 14 in one
//! undo step; undone, the file is as before.

use egui_kittest::kittest::Queryable;

use crate::layout::Panel;
use crate::tables::TENSION_NO_COMPRESSION;
use crate::tests::{copy_of, harness};

fn text(h: &egui_kittest::Harness<'_, crate::app::StructureApp>) -> String {
    h.state().file.as_ref().unwrap().text()
}

#[test]
fn the_no_compression_switch_writes_the_key_and_undoes() {
    let path = copy_of("type-02.ohls", "tension-compression");
    let mut h = harness(&path);
    crate::layout::open_panel(&mut h.state_mut().dock, Panel::MemberSections);
    h.run();
    let start = text(&h);
    assert!(!start.contains("tension_compression"), "{start}");
    assert!(start.contains("version = 8"), "{start}");
    h.get_by_label(TENSION_NO_COMPRESSION).click();
    h.run();
    let ticked = text(&h);
    assert!(
        ticked.contains("tension_compression = \"none\"") && ticked.contains("version = 14"),
        "{ticked}"
    );
    // The structure as read takes it.
    let t = h.state().file.as_ref().unwrap().structure().clone();
    assert_eq!(
        t.analysis
            .as_ref()
            .and_then(|a| a.get("tension_compression"))
            .and_then(toml::Value::as_str),
        Some("none")
    );
    // One undo step: the file as it was.
    h.state_mut().undo();
    h.run();
    assert_eq!(text(&h), start);
    // Ticked and unticked: the key goes, the version stays.
    h.get_by_label(TENSION_NO_COMPRESSION).click();
    h.run();
    h.get_by_label(TENSION_NO_COMPRESSION).click();
    h.run();
    let back = text(&h);
    assert!(!back.contains("tension_compression"), "{back}");
    assert!(back.contains("version = 14"), "{back}");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
