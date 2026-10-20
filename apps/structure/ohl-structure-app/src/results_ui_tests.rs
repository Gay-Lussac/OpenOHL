// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The FE results panel on an extension, with the pole check, and the
//! structure report, driven as a user would (egui_kittest).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::StructureApp;
use crate::tests::{copy_of, harness, menu};

/// A 10 m frame column with its own section table, fixed at its foot, a
/// +2 m extension piece under it, and a case of 1 kN across at the top
/// (no self-weight).
const COLUMN: &str = r#"format = "ohl-structure"
version = 3
id = "col"
name = "column"
kind = "other"

[[point]]
label = "A1"
node = "N1"

[[material]]
id = "steel"
elasticity = "210 GPa"
density = "7850 kg/m3"
yield = "355 MPa"

[[section]]
id = "tube"
material = "steel"
area = "6000 mm2"
iy = "2700 cm4"
iz = "2700 cm4"
torsion = "5400 cm4"
i_min = "2700 cm4"

[[node]]
id = "N0"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N1"
point = ["0 m", "0 m", "10 m"]

[[member]]
id = "m1"
nodes = ["N0", "N1"]
section = "tube"
kind = "frame"

[[extension]]
name = "+2"
dz = "2 m"

[[extension.node]]
id = "E0"
point = ["0 m", "0 m", "-2 m"]
support = "fixed"

[[extension.member]]
id = "e1"
nodes = ["E0", "N0"]
section = "tube"
kind = "frame"

[[load_case]]
name = "across"
self_weight = 0

[[load_case.load]]
value = "1 kN"
direction = "x"
points = ["A1"]
"#;

fn written(name: &str, text: &str, tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ohl-structure-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let to = dir.join(name);
    std::fs::write(&to, text).unwrap();
    to
}

/// Runs until the run going on ends.
fn wait(h: &mut Harness<'static, StructureApp>) {
    let started = Instant::now();
    loop {
        h.step();
        if !h.state().results.running() {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "no result in 60 s"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    h.run();
}

/// The foot's moment of the case: as written 1 kN × 10 m = 10 kN·m at
/// N0; on +2 (chosen in the panel, the run starts again by itself) the
/// piece's foot E0 carries 1 kN × 12 m = 12 kN·m and N0 no support. The
/// header says which.
#[test]
fn results_follow_the_extension_chosen() {
    let path = written("col.ohls", COLUMN, "results-ext");
    let mut h = harness(&path);
    menu(&mut h, "Results", "FE results…");
    h.get_by_label("▶ Run").click();
    wait(&mut h);
    let moment = |h: &Harness<'static, StructureApp>| {
        let run = h.state().results.run().unwrap().clone();
        let r = run.cases[0].fe.clone().unwrap().unwrap();
        assert_eq!(r.reactions.len(), 1);
        (
            run.extension.clone(),
            r.reactions[0].joint.clone(),
            r.reactions[0].moment[1],
        )
    };
    let (ext, joint, m) = moment(&h);
    assert_eq!((ext, joint.as_str()), (None, "N0"));
    assert!((m + 10_000.0).abs() < 1e-3, "{m}");
    h.get_by_label("Solved as written");
    // The extension chosen (as its combo box does): solved again.
    h.state_mut().ext.shown = Some("+2".into());
    h.run();
    assert!(h.state().results.running() || h.state().results.ran_ext.is_some());
    wait(&mut h);
    let (ext, joint, m) = moment(&h);
    assert_eq!((ext.as_deref(), joint.as_str()), (Some("+2"), "E0"));
    assert!((m + 12_000.0).abs() < 1e-3, "{m}");
    h.get_by_label("Solved on extension +2 (the structure 2 m higher)");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// The example pole structure (its .ohlp beside it) with a value typed
/// at A1: the pole check in the panel, its usage per check, then the
/// structure report written with the members' CSV beside it.
#[test]
fn the_pole_check_and_the_report() {
    let path = copy_of("type-05.ohls", "results-pole");
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../examples/synthetic-line/Structures/steel-pole-30m.ohlp"),
        dir.join("steel-pole-30m.ohlp"),
    )
    .unwrap();
    let mut h = harness(&path);
    let own = ohl_structure_loads::LoadRow {
        case: "typed".into(),
        load: None,
        at: ohl_structure_loads::At::Point,
        label: "A1".into(),
        force: [5000.0, 1000.0, 8000.0],
        source: ohl_structure_loads::Source::Typed,
        evaluated: None,
        missing: None,
        note: String::new(),
        area: None,
    };
    h.state_mut().loads.type_value(&own, own.force);
    h.run();
    menu(&mut h, "Results", "FE results…");
    assert!(!h.state().can_write_report());
    h.get_by_label("▶ Run").click();
    wait(&mut h);
    let run = h.state().results.run().unwrap().clone();
    let c = &run.cases[0];
    assert!(c.fe.is_none());
    let p = c.pole.clone().unwrap().unwrap();
    // First order: 26 m × √(5² + 1²) kN at the ground line.
    assert!((p.ground_moment - 26.0 * 5000f64.hypot(1000.0)).abs() < 1e-6);
    h.get_by_label("pole");
    h.get_by_label_contains("Pole check — ");
    assert!(h.state().can_write_report());
    let out = dir.join("type-05-report.html");
    let said = h.state_mut().write_report(&out).unwrap();
    assert!(said.contains("type-05-report.members.csv"), "{said}");
    let html = std::fs::read_to_string(&out).unwrap();
    assert!(html.contains(ohl_structure_export::report::WARNING));
    assert!(html.contains(crate::VERSION));
    assert!(html.contains("<h2>Pole check</h2>"));
    assert!(dir.join("type-05-report.members.csv").exists());
    std::fs::remove_dir_all(&dir).ok();
}
