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

/// D14 (the owner's decision of 2026-11-19): D1's bolts with 70 mm holes
/// (wider than the leg's 61 mm flange) placed only as the joint details
/// assume them (its design data's bolts, no connection at the end): the
/// leg's group "does not fit as assumed — not detailed" — failing, no
/// usage, counted apart from a joint that does not fit as given; the same
/// holes at an end given (its connection's gauges and distances): "does not
/// fit as given".
#[test]
fn a_misfit_only_where_assumed_reads_not_detailed() {
    if !built("asce10") {
        return;
    }
    let assumed = TOWER.replace(
        "bolts = { diameter = \"16 mm\", count = 1 }",
        "bolts = { diameter = \"16 mm\", count = 2, hole = \"70 mm\" }",
    );
    assert_ne!(assumed, TOWER);
    let r = rows(&assumed, &axial(40e3));
    let leg = r
        .iter()
        .find(|x| x.part == PlatePart::Bearing && x.does_not_fit.is_some())
        .unwrap_or_else(|| panic!("{r:?}"));
    assert!(leg.misfit_assumed, "{leg:?}");
    assert!(leg.usage.is_none(), "{leg:?}");
    assert!(
        leg.governs.starts_with(crate::DOES_NOT_FIT_ASSUMED),
        "{}",
        leg.governs
    );
    let n = crate::joints_not_fitting_with(&r);
    assert!(n.get("N2").is_some_and(|m| m.assumed), "{n:?}");
    assert!(
        crate::joints_not_fitting(&r).contains_key("N2"),
        "still failing"
    );
    // Given at the end: as given.
    let given = format!(
        "{TOWER}{}",
        AT_N2.replace("grade = \"8.8\" }", "grade = \"8.8\", hole = \"70 mm\" }")
    );
    let r = rows(&given, &axial(40e3));
    let leg = r
        .iter()
        .find(|x| x.part == PlatePart::Bearing && x.does_not_fit.is_some())
        .unwrap_or_else(|| panic!("{r:?}"));
    assert!(!leg.misfit_assumed, "{leg:?}");
    assert!(
        leg.governs.starts_with(crate::DOES_NOT_FIT)
            && !leg.governs.starts_with(crate::DOES_NOT_FIT_ASSUMED),
        "{}",
        leg.governs
    );
    let n = crate::joints_not_fitting_with(&r);
    assert!(n.get("N2").is_some_and(|m| !m.assumed), "{n:?}");
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
/// the leg's flange's or the plate's bending governs, never said "not
/// checked"; the plate's own prying and bending worked out (2026-11-19, a
/// flange without a web), no longer said not worked out.
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
                && !hold.governs.contains("the prying of plate G1"),
            "{}",
            hold.governs
        );
        // The plate's bending governs here: b′ 42 mm from the holding bolt
        // to D1's farther bolt, less half the bolt.
        assert!(
            hold.governs.contains("bending of plate G1") && hold.governs.contains("b′ 42 mm"),
            "{code}: {}",
            hold.governs
        );
    }
}

/// The bolt grade set by command (owner 2026-11-19): a library bolt
/// without strengths (as a tower program's export gives them) leaves the
/// plate's holding bolts' tension not checked, said NOT ON THE SAFE SIDE;
/// SIA 263:2013 Table 2's 8.8 set on the members' bolts gives them its
/// f_ub — the tension checked — and the group says the grade, set by
/// command, and where its values come from.
#[test]
fn a_grade_set_by_command_checks_the_bolts_in_tension() {
    if !built("asce10") {
        return;
    }
    let bare = ohl_structures::bolts::BoltLibrary::parse(
        "format = \"ohl-bolts\"\nversion = 1\n\n[[bolt]]\nid = \"B16\"\n\
         diameter = \"16 mm\"\nhole = \"17.5 mm\"\n",
    )
    .unwrap();
    let base = held("B16").replace(", grade = \"8.8\"", "");
    let mut graded = base.clone();
    for m in ["L1", "L2", "D1"] {
        graded = graded.replacen(
            &format!("id = \"{m}\"\n"),
            &format!("id = \"{m}\"\nbolt_grade = \"8.8\"\n"),
            1,
        );
    }
    graded.push_str(
        "\n[[bolt_grade]]\ngrade = \"8.8\"\nultimate = \"800 MPa\"\nyield = \"640 MPa\"\n\
         source = \"SIA 263:2013, 3.4.2, Table 2\"\nset_by = \"command\"\n",
    );
    let hold = |text: &str| {
        let plans = plate_plans(text, &library(), &bare).unwrap();
        let mut ends = crate::EndForces::new();
        ends.insert("D1".into(), [[0.0; 6], [0.8e3, 30e3, 0.6e3, 0.0, 0.0, 0.0]]);
        let forces = axial(1e3);
        crate::plate_usage_cases_with(&plans, &[("one", 1.0, &forces)], &[&ends])
            .remove(0)
            .into_iter()
            .find(|x| matches!(&x.part, PlatePart::Holding(_)))
            .unwrap()
    };
    let before = hold(&base);
    assert!(
        before.governs.contains("bolt tension not checked"),
        "{}",
        before.governs
    );
    let after = hold(&graded);
    assert!(
        !after.governs.contains("bolt tension not checked"),
        "{}",
        after.governs
    );
    assert!(
        after.notes.iter().any(|n| n.contains(
            "bolt grade 8.8 set by command: f_ub 800 MPa, f_yb 640 MPa (SIA 263:2013, 3.4.2, \
             Table 2)"
        )),
        "{:?}",
        after.notes
    );
}

/// A bolt library with one bolt giving a tension capacity (invented).
fn bolts() -> ohl_structures::bolts::BoltLibrary {
    ohl_structures::bolts::BoltLibrary::parse(
        "format = \"ohl-bolts\"\nversion = 1\n\n[[bolt]]\nid = \"X17\"\n\
         diameter = \"16 mm\"\nultimate = \"800 MPa\"\ntension_capacity = \"120 kN\"\n",
    )
    .unwrap()
}

/// The invented tower with a horizontal H1 ending at N2 (along +x, in the
/// face y = 1 m), bolted onto the leg L2 by two M16, and a plan member P1
/// (level, at 45° to H1) laid onto H1 near its end by two M16 (L14).
fn laid_onto_h1() -> String {
    let tower = TOWER.replace(
        "[[member]]\nid = \"L1\"",
        "[[node]]\nid = \"N5\"\npoint = [\"1.5 m\", \"1 m\", \"5 m\"]\nsupport = \"fixed\"\n\n\
         [[node]]\nid = \"N6\"\npoint = [\"1.5 m\", \"2.5 m\", \"5 m\"]\nsupport = \"fixed\"\n\n\
         [[member]]\nid = \"H1\"\nnodes = [\"N2\", \"N5\"]\nsection = \"EA-61x7\"\n\
         design = { class = \"bracing\" }\n\n\
         [[member]]\nid = \"P1\"\nnodes = [\"N6\", \"N2\"]\nsection = \"EA-61x7\"\n\
         design = { class = \"bracing\" }\n\n\
         [[member]]\nid = \"L1\"",
    );
    assert_ne!(tower, TOWER);
    format!(
        "{tower}\n[[connection]]\nmember = \"H1\"\njoint = \"N2\"\nonto = \"L2\"\n\
         bolts = {{ count = 2, diameter = \"16 mm\", grade = \"8.8\" }}\n\
         gauges = [\"30 mm\"]\ngauge_lines = 1\npitch = \"50 mm\"\nend_distance = \"30 mm\"\n\
         \n[[connection]]\nmember = \"P1\"\njoint = \"N2\"\nonto = \"H1\"\n\
         bolts = {{ count = 2, diameter = \"16 mm\", grade = \"8.8\" }}\n\
         gauges = [\"30 mm\"]\ngauge_lines = 1\npitch = \"50 mm\"\nend_distance = \"30 mm\"\n"
    )
}

/// L14 (the owner's answer of 2026-11-22 to L13's question 3): a member
/// ending at the joint with an end laid onto it — its own end bolts carry
/// that end's force too. H1's end bolts get a group of their own (shear,
/// bearing on H1) under H1's and P1's forces; the leg's bearing under H1's
/// bolts follows the force through (more with P1 pulling than without);
/// (L14's question 2) H1's own steel at those bolts checked under the
/// larger of its own force and the resultant (by hand); P1 at no force:
/// H1's own force alone.
#[test]
fn a_members_end_bolts_carry_the_ends_laid_onto_it() {
    if !built("asce10") {
        return;
    }
    let text = laid_onto_h1();
    let with = |p1: f64| {
        let f: BTreeMap<String, f64> = [
            ("D1", 0.0),
            ("L1", -100e3),
            ("L2", -90e3),
            ("H1", 30e3),
            ("P1", p1),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        rows(&text, &f)
    };
    let r0 = with(0.0);
    let r1 = with(20e3);
    let own = |r: &[PlateUsage]| {
        r.iter()
            .find(|x| x.part == PlatePart::EndBolts("P1".into()))
            .cloned()
            .unwrap_or_else(|| panic!("{r:?}"))
    };
    let (e0, e1) = (own(&r0), own(&r1));
    assert_eq!(e1.plate, "H1");
    assert_eq!(e1.joint, "N2");
    assert!(e1.usage.is_some(), "{e1:?}");
    // H1 alone: 15 kN a bolt; with P1 more.
    assert!((e0.force - 15e3).abs() < 1e-6, "{e0:?}");
    assert!(e1.force > e0.force + 1e3, "{e0:?} {e1:?}");
    assert!(e1.usage.unwrap() > e0.usage.unwrap(), "{e0:?} {e1:?}");
    let leg = |r: &[PlateUsage]| {
        r.iter()
            .find(|x| x.part == PlatePart::Bearing && x.plate == "L2")
            .cloned()
            .unwrap_or_else(|| panic!("{r:?}"))
    };
    let (l0, l1) = (leg(&r0), leg(&r1));
    assert!(l1.force > l0.force + 1e3, "{l0:?} {l1:?}");
    assert!(
        l1.notes.iter().any(|n| n.contains("H1 with P1")),
        "{:?}",
        l1.notes
    );
    // (L14's question 2, the owner 2026-11-22) H1's own steel at its end
    // bolts: checked under the larger of its own force and the resultant —
    // nothing said NOT ON THE SAFE SIDE there any more.
    assert!(
        !r1.iter()
            .any(|x| matches!(&x.part, PlatePart::NotChecked(w) if w.contains("H1's own steel"))),
        "{r1:?}"
    );
    let steel = |r: &[PlateUsage]| {
        r.iter()
            .find(|x| x.part == PlatePart::EndSteel("P1".into()))
            .cloned()
            .unwrap_or_else(|| panic!("{r:?}"))
    };
    let (s0, s1) = (steel(&r0), steel(&r1));
    assert_eq!(s1.plate, "H1");
    // By hand: H1 along +x at 30 kN, P1 at 45° in the level plane at 20 kN:
    // (30 + 20 cos 45°, 20 sin 45°) = (44.142, 14.142) kN, 46.352 kN;
    // P1 at no force: H1's own 30 kN.
    assert!((s1.force - 46_352.0).abs() < 1.0, "{s1:?}");
    assert!((s0.force - 30_000.0).abs() < 1e-6, "{s0:?}");
    let cap = crate::member_capacity(&text, &library(), &Default::default(), "H1")
        .unwrap()
        .tension
        .expect("a tension capacity")
        .0;
    assert!(
        (s1.usage.unwrap() - s1.force / cap).abs() < 1e-9,
        "{s1:?} {cap}"
    );
    assert!(
        !s1.governs.contains(ohl_structures_fe::check::NOT_SAFE),
        "{}",
        s1.governs
    );
}

/// L14's question 2 (the owner 2026-11-22: "check the impact on the bolt on
/// the other member to make it safe"): H1 ending at N2 bolted onto a plate
/// on the leg, P1 laid onto H1 near its end — the plate's own checks under
/// H1's bolts take the larger of H1's force and the resultant through them
/// (by hand 46.352 kN with P1 at 20 kN; H1's own 30 kN with P1 at none,
/// and with P1 pushing back the other way, never less than H1's own); a
/// resultant turned more than 15° off H1's line said not on the safe side
/// (the plate's sections taken across H1's line).
#[test]
fn a_plate_under_a_members_end_bolts_takes_the_force_followed_through() {
    if !built("asce10") {
        return;
    }
    let text = laid_onto_h1().replace(
        "member = \"H1\"\njoint = \"N2\"\nonto = \"L2\"",
        "member = \"H1\"\njoint = \"N2\"\nonto = \"G1\"",
    ) + "\n[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"8 mm\"\non = \"L2\"\n";
    let with = |h1: f64, p1: f64| {
        let f: BTreeMap<String, f64> = [
            ("D1", 0.0),
            ("L1", -100e3),
            ("L2", -90e3),
            ("H1", h1),
            ("P1", p1),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        rows(&text, &f)
    };
    let on_plate = |r: &[PlateUsage]| {
        r.iter()
            .find(|x| x.plate == "G1" && x.part == PlatePart::Member("H1".into()))
            .cloned()
            .unwrap_or_else(|| panic!("{r:?}"))
    };
    let r0 = on_plate(&with(30e3, 0.0));
    let r1 = on_plate(&with(30e3, 20e3));
    let r2 = on_plate(&with(30e3, -5e3));
    assert!((r0.force - 30_000.0).abs() < 1e-6, "{r0:?}");
    assert!((r1.force - 46_352.0).abs() < 1.0, "{r1:?}");
    // P1 at −5 kN: (30 − 3.536, −3.536) = 26.70 kN < 30 kN — H1's own.
    assert!((r2.force - 30_000.0).abs() < 1e-6, "{r2:?}");
    assert!(
        r1.notes
            .iter()
            .any(|n| n.contains("H1 with P1 laid onto it")),
        "{:?}",
        r1.notes
    );
    // The resultant turns atan(14.142 / 44.142) = 17.8° off H1's line, more
    // than 15°: the plate's sections taken across H1's line, said not on
    // the safe side; P1 alone pulling (H1 at no force): along P1, 45° off.
    let unsafe_mark = ohl_structures_fe::check::NOT_SAFE;
    assert!(
        r1.governs.contains(unsafe_mark) && r1.governs.contains("turns 18°"),
        "{}",
        r1.governs
    );
    assert!(!r0.governs.contains(unsafe_mark), "{}", r0.governs);
    let r3 = on_plate(&with(0.0, 20e3));
    assert!((r3.force - 20_000.0).abs() < 1.0, "{r3:?}");
    assert!(r3.governs.contains("turns 45°"), "{}", r3.governs);
}
