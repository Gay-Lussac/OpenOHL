// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The joints' own checks end to end (stage 4.5, 2026-11-19; invented
//! towers): the groups gathered from the joint details, checked by the
//! bundled plugins `asce10` and `sia263`.

use std::collections::BTreeMap;

use crate::joints_tests::{TOWER, library};
use crate::{PlatePart, PlateUsage, plate_plans, plate_usage_cases};

/// D1 lapped onto the leg at N2 by two M16 (8.8) on one gauge line.
const AT_N2: &str = r#"
[[connection]]
member = "D1"
joint = "N2"
leg = "long"
bolts = { count = 2, diameter = "16 mm", grade = "8.8" }
gauges = ["30 mm"]
gauge_lines = 1
pitch = "50 mm"
end_distance = "30 mm"
"#;

fn axial(d1: f64) -> BTreeMap<String, f64> {
    [("D1", d1), ("L1", -100e3), ("L2", -90e3)]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}

fn built(name: &str) -> bool {
    let wasm = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../../plugins/codes/{name}/target/wasm32-wasip2/release/{name}.wasm"
    ));
    if wasm.is_file() {
        return true;
    }
    assert!(
        std::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "plugins/codes/{name} not built"
    );
    eprintln!("skipping: plugins/codes/{name} not built");
    false
}

fn rows(text: &str, forces: &BTreeMap<String, f64>) -> Vec<PlateUsage> {
    let plans = plate_plans(text, &library(), &Default::default()).unwrap();
    plate_usage_cases(&plans, &[("one", 1.0, forces)]).remove(0)
}

/// The leg's bearing under a diagonal lapped onto it (ASCE 10-97, the
/// default): one group for the leg D1 bolts onto (L2: L1 and L2 in line
/// but not said continuous, so each ends at the joint — owner 2026-11-19,
/// on the safe side, said), its two bolts each carrying half of D1's force
/// (ASCE 10-97 4.1), the most loaded named with its check and clause;
/// twice the force, twice the usage (the code's checks are linear in it
/// until a distance governs).
#[test]
fn a_diagonal_lapped_onto_the_leg_gives_the_legs_bearing() {
    if !built("asce10") {
        return;
    }
    let text = format!("{TOWER}{AT_N2}");
    let r = rows(&text, &axial(40e3));
    let leg = r
        .iter()
        .find(|x| x.part == PlatePart::Bearing)
        .unwrap_or_else(|| panic!("{r:?}"));
    assert_eq!(leg.joint, "N2");
    assert_eq!(leg.plate, "L2");
    assert!((leg.force - 20e3).abs() < 1e-6, "{}", leg.force);
    let u = leg.usage.unwrap_or_else(|| panic!("{leg:?}"));
    // Bearing on the 7 mm leg (4.4; the bolt's F_u not given: the leg's):
    // 20 kN / (1.5 F_u × 16 mm × 7 mm); the toe and D1's other bolt far
    // enough (the leg's end at the joint not given: its end distance not
    // checked, said).
    let fu = library()
        .material("steel-example")
        .unwrap()
        .ultimate
        .unwrap();
    assert!(
        (u - 20e3 / (1.5 * fu * 0.016 * 0.007)).abs() < 1e-9,
        "{u} {leg:?}"
    );
    assert!(
        leg.governs.contains("bearing on L2 (ASCE 10-97 4.4)"),
        "{}",
        leg.governs
    );
    assert!(leg.governs.contains(" of 2 (carries D1"), "{}", leg.governs);
    assert!(
        leg.notes
            .iter()
            .any(|n| n.contains("not said continuous: each taken as ending at the joint")),
        "{:?}",
        leg.notes
    );
    let r2 = rows(&text, &axial(80e3));
    let leg2 = r2.iter().find(|x| x.part == PlatePart::Bearing).unwrap();
    let ratio = leg2.usage.unwrap() / u;
    assert!(
        (ratio - 2.0).abs() < 1e-9 || leg2.governs.contains("distance"),
        "{ratio}"
    );
    // The joint's usage: the largest of its checks.
    let j = crate::joint_plate_usage(&r);
    assert_eq!(j.get("N2").copied(), Some(u));
}

/// The same by SIA 263 (the bundled `sia263`): bearing (76) with e_1
/// along each bolt's force in the leg, the bolt shear of Table 16.
#[test]
fn the_legs_bearing_by_sia_263() {
    if !built("sia263") {
        return;
    }
    let text = format!("{TOWER}{AT_N2}\n[analysis]\ncapacity = \"sia263-2013\"\n");
    let r = rows(&text, &axial(-40e3));
    let leg = r.iter().find(|x| x.part == PlatePart::Bearing).unwrap();
    assert!(leg.usage.is_some(), "{leg:?}");
    assert!(leg.governs.contains("SIA 263"), "{}", leg.governs);
}

/// A plate that the joint's leg does not bolt onto: how it is held is not
/// given — said, without a usage, never passed.
#[test]
fn a_plate_not_held_by_the_leg_is_said() {
    let text = format!(
        "{TOWER}\n[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         \n[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"G1\"\n\
         bolts = {{ count = 2, diameter = \"16 mm\", grade = \"8.8\" }}\n"
    );
    let plans = plate_plans(&text, &library(), &Default::default()).unwrap();
    let r = crate::plate_usage(&plans, &axial(40e3), 1.0);
    let said = r
        .iter()
        .find(|x| matches!(&x.part, PlatePart::NotChecked(_)))
        .unwrap_or_else(|| panic!("{r:?}"));
    assert_eq!(said.plate, "G1");
    assert!(said.usage.is_none());
    assert!(
        said.governs
            .starts_with("NOT CHECKED, NOT ON THE SAFE SIDE")
            && said.governs.contains("how plate G1 is held"),
        "{}",
        said.governs
    );
}

/// A plate held by the leg's bolts (L1 bolted onto G1 by two M16, D1 onto
/// G1 too): its holding group — L1's bolts on the plate carrying D1's
/// force with its moment — and the leg's bearing under those bolts.
#[test]
fn a_plate_held_by_the_leg_gives_its_holding_bolts() {
    if !built("asce10") {
        return;
    }
    let text = format!(
        "{TOWER}\n[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         \n[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"G1\"\nleg = \"long\"\n\
         bolts = {{ count = 2, diameter = \"16 mm\", grade = \"8.8\" }}\n\
         gauges = [\"30 mm\"]\ngauge_lines = 1\npitch = \"50 mm\"\nend_distance = \"30 mm\"\n\
         \n[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nonto = \"G1\"\n\
         bolts = {{ count = 2, diameter = \"16 mm\", grade = \"8.8\" }}\n\
         gauges = [\"30 mm\"]\ngauge_lines = 1\npitch = \"60 mm\"\nend_distance = \"40 mm\"\n"
    );
    let r = rows(&text, &axial(40e3));
    let hold = r
        .iter()
        .find(|x| matches!(&x.part, PlatePart::Holding(_)))
        .unwrap_or_else(|| panic!("{r:?}"));
    assert_eq!(hold.plate, "G1");
    assert_eq!(hold.part, PlatePart::Holding("L1".into()));
    // At least D1's force shared by the two (more with its moment).
    assert!(hold.force >= 20e3 - 1e-6, "{}", hold.force);
    assert!(hold.usage.is_some(), "{hold:?}");
    assert!(hold.governs.contains("ASCE 10-97"), "{}", hold.governs);
    let bearing = r
        .iter()
        .find(|x| x.part == PlatePart::Bearing && x.plate.starts_with("L1"))
        .unwrap_or_else(|| panic!("{r:?}"));
    assert!(
        (bearing.force - hold.force).abs() < 1e-6,
        "{bearing:?} {hold:?}"
    );
}

/// L1 and L2 said one piece through N2 (`continuous`): one group for the
/// leg running on, without the note of pieces ending there.
#[test]
fn a_leg_said_continuous_runs_on() {
    if !built("asce10") {
        return;
    }
    let text = format!(
        "{TOWER}{AT_N2}\n[[connection]]\nmember = \"L1\"\njoint = \"N2\"\n\
         kind = \"continuous\"\nonto = \"L2\"\n"
    );
    let r = rows(&text, &axial(40e3));
    let leg = r
        .iter()
        .find(|x| x.part == PlatePart::Bearing)
        .unwrap_or_else(|| panic!("{r:?}"));
    assert_eq!(leg.plate, "L1/L2");
    assert!(
        !leg.notes.iter().any(|n| n.contains("not said continuous")),
        "{:?}",
        leg.notes
    );
    assert!(leg.usage.is_some() && leg.does_not_fit.is_none(), "{leg:?}");
}

/// The leg running on through N2 (L1 into L2), a gusset plate G1 on it
/// held by its own two bolts onto L1 (`[plate.leg_bolts]`), D1 bolted onto
/// G1 by its short leg (lying in the face y = 1 m); `bolt` names the
/// bolts' library bolt ("" none).
fn held(bolt: &str) -> String {
    let b = if bolt.is_empty() {
        String::new()
    } else {
        format!(", bolt = \"{bolt}\"")
    };
    format!(
        "{TOWER}\n[[connection]]\nmember = \"L1\"\njoint = \"N2\"\n\
         kind = \"continuous\"\nonto = \"L2\"\n\
         \n[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"G1\"\nleg = \"short\"\n\
         bolts = {{ count = 2, diameter = \"16 mm\", grade = \"8.8\"{b} }}\n\
         gauges = [\"28 mm\"]\ngauge_lines = 1\npitch = \"50 mm\"\nend_distance = \"30 mm\"\n\
         \n[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         \n[plate.leg_bolts]\nmember = \"L1\"\n\
         bolts = {{ count = 2, diameter = \"16 mm\", grade = \"8.8\"{b} }}\n\
         bolt_points = [[\"-60 mm\", \"30 mm\"], [\"60 mm\", \"30 mm\"]]\n"
    )
}

/// A gusset plate on a leg running on through the joint, held by its own
/// bolts onto the leg (`[plate.leg_bolts]`, 2026-11-19): checked as the
/// plate's holding bolts (D1's force on them) and the leg's bearing under
/// them, no longer "not given".
#[test]
fn a_plate_on_a_leg_running_on_is_held_by_its_own_bolts() {
    if !built("asce10") {
        return;
    }
    let r = rows(&held(""), &axial(40e3));
    assert!(
        !r.iter()
            .any(|x| matches!(&x.part, PlatePart::NotChecked(w) if w.contains("how plate G1"))),
        "{r:?}"
    );
    let hold = r
        .iter()
        .find(|x| matches!(&x.part, PlatePart::Holding(_)))
        .unwrap_or_else(|| panic!("{r:?}"));
    assert_eq!(hold.plate, "G1");
    assert_eq!(hold.part, PlatePart::Holding("L1".into()));
    assert!(hold.force >= 20e3 - 1e-6, "{}", hold.force);
    assert!(hold.usage.is_some(), "{hold:?}");
    assert!(
        hold.notes.iter().any(|n| n.contains("[plate.leg_bolts]")),
        "{:?}",
        hold.notes
    );
    let bearing = r
        .iter()
        .find(|x| x.part == PlatePart::Bearing && x.plate == "L1/L2")
        .unwrap_or_else(|| panic!("{r:?}"));
    assert!(
        (bearing.force - hold.force).abs() < 1e-6,
        "{bearing:?} {hold:?}"
    );
}

/// D1's two bolts 10 mm apart (17.5 mm holes): its holes in the leg
/// overlap — the leg's group does not fit: no usage, failing, counted
/// apart (never an infinite usage).
#[test]
fn overlapping_holes_do_not_fit() {
    if !built("asce10") {
        return;
    }
    let text = format!(
        "{TOWER}{}",
        AT_N2
            .replace("\"50 mm\"", "\"10 mm\"")
            .replace("grade = \"8.8\" }", "grade = \"8.8\", hole = \"17.5 mm\" }")
    );
    let r = rows(&text, &axial(40e3));
    let leg = r
        .iter()
        .find(|x| x.part == PlatePart::Bearing)
        .unwrap_or_else(|| panic!("{r:?}"));
    assert!(leg.usage.is_none(), "{leg:?}");
    assert!(
        leg.does_not_fit
            .as_deref()
            .is_some_and(|w| w.contains("overlap")),
        "{leg:?}"
    );
    assert!(
        leg.governs.starts_with(crate::DOES_NOT_FIT),
        "{}",
        leg.governs
    );
    let n = crate::joints_not_fitting(&r);
    assert!(n.get("N2").is_some_and(|w| w.contains("overlap")), "{n:?}");
    assert!(crate::joint_plate_usage(&r).values().all(|u| u.is_finite()));
}

/// D1 pulled out of the plate's plane (its end force square to the plies,
/// from the analysis): the plate's holding bolts in tension checked by the
/// plugin — with a small force in the plane, the tension, the prying or
/// the leg's flange's bending governs, never said "not checked"; the
/// plate's own bending (not worked out) said not on the safe side.
#[test]
fn bolts_pulled_out_of_the_plane_are_checked_in_tension() {
    for (code, capacity) in [
        ("asce10", ""),
        ("sia263", "\n[analysis]\ncapacity = \"sia263-2013\"\n"),
    ] {
        if !built(code) {
            return;
        }
        let text = format!("{}{capacity}", held("X17"));
        let plans = plate_plans(&text, &library(), &bolts()).unwrap();
        let mut ends = crate::EndForces::new();
        // On D1 at N2 (its second end): 1 kN along it, 30 kN square to
        // the face y = 1 m its leg lies in.
        ends.insert("D1".into(), [[0.0; 6], [0.8e3, 30e3, 0.6e3, 0.0, 0.0, 0.0]]);
        let forces = axial(1e3);
        let r = crate::plate_usage_cases_with(&plans, &[("one", 1.0, &forces)], &[&ends]).remove(0);
        let hold = r
            .iter()
            .find(|x| matches!(&x.part, PlatePart::Holding(_)))
            .unwrap_or_else(|| panic!("{r:?}"));
        assert!(hold.usage.is_some(), "{code}: {hold:?}");
        assert!(
            hold.governs.contains("tension") || hold.governs.contains("bending of"),
            "{code}: {}",
            hold.governs
        );
        assert!(
            !hold.governs.contains("bolts in tension not checked")
                && hold.governs.contains("the prying of plate G1"),
            "{}",
            hold.governs
        );
    }
}

/// A bolt library with one bolt giving a tension capacity (invented).
fn bolts() -> ohl_structures::bolts::BoltLibrary {
    ohl_structures::bolts::BoltLibrary::parse(
        "format = \"ohl-bolts\"\nversion = 1\n\n[[bolt]]\nid = \"X17\"\n\
         diameter = \"16 mm\"\nultimate = \"800 MPa\"\ntension_capacity = \"120 kN\"\n",
    )
    .unwrap()
}
