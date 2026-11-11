// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The plates' and welds' checks per case, on a welded plate (invented
//! data).

use std::collections::BTreeMap;

use crate::joints::joint_details;
use crate::joints_tests::{OTHER_FACE, TOWER, library};
use crate::{PlatePart, plate_plans, plate_usage};

/// A welded plate of G1 (D1 onto it) and G2 (D2), a weld of a = 6 mm
/// given on G2 (`[[plate.weld]]`), its length the seam's.
fn welded() -> String {
    format!(
        "{TOWER}{OTHER_FACE}\n[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         \n[[plate]]\nid = \"G2\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         [[plate.weld]]\nto = \"G1\"\nsize = \"6 mm\"\n\
         \n[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"G1\"\n\
         \n[[connection]]\nmember = \"D2\"\njoint = \"N2\"\nonto = \"G2\"\n"
    )
}

fn axial() -> BTreeMap<String, f64> {
    [("D1", 40e3), ("D2", -25e3), ("L1", -100e3), ("L2", -90e3)]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}

/// Under SIA 263 each member's bolts are checked in its plate (bearing,
/// block shear, sections, distances) and the weld by (87) and (88) under
/// the larger resultant of its two sides (D1's 40 kN against D2's 25
/// kN); the strength factor scales the resistances.
#[test]
fn plates_and_welds_are_checked_by_sia263() {
    let sia = format!("{}\n[analysis]\ncapacity = \"sia263-2013\"\n", welded());
    let d = joint_details(&sia, &library(), &Default::default()).unwrap();
    let w = &d.joint("N2").unwrap().plates[0].welds[0];
    assert_eq!(w.size, Some(0.006));
    assert_eq!(w.label().as_deref(), Some("a6"));
    let plans = plate_plans(&sia, &library(), &Default::default()).unwrap();
    let u = plate_usage(&plans, &axial(), 1.0);
    let d1 = u
        .iter()
        .find(|p| p.part == PlatePart::Member("D1".into()))
        .unwrap();
    assert_eq!(d1.plate, "G1");
    assert!(d1.usage.is_some_and(|x| x > 0.0 && x < 1.0), "{d1:?}");
    for c in ["(76)", "(83)", "(39)", "e_1"] {
        assert!(d1.checks.iter().any(|x| x.0.contains(c)), "{c}: {d1:?}");
    }
    let d2 = u
        .iter()
        .find(|p| p.part == PlatePart::Member("D2".into()))
        .unwrap();
    assert!(
        d2.notes
            .iter()
            .any(|n| n.contains("plate buckling not checked"))
    );
    let weld = u
        .iter()
        .find(|p| matches!(&p.part, PlatePart::Weld(_)))
        .unwrap();
    assert!((weld.force - 40e3).abs() < 1e-6, "{weld:?}");
    assert!(weld.governs.contains("(87)"), "{weld:?}");
    assert!(weld.notes.iter().any(|n| n.contains("the seam's")));
    let half = plate_usage(&plans, &axial(), 0.5);
    let w2 = half
        .iter()
        .find(|p| matches!(&p.part, PlatePart::Weld(_)))
        .unwrap();
    assert!((w2.usage.unwrap() - 2.0 * weld.usage.unwrap()).abs() < 1e-12);
    // The worst per joint.
    let j = crate::joint_plate_usage(&u);
    assert!(j["N2"] >= weld.usage.unwrap());
}

/// Under ASCE 10-97 (the default) the weld is not checked (the standard
/// has no weld rule), said; the bolts are (4.4 …); the simple check
/// checks nothing, said.
#[test]
fn asce10_has_no_weld_rule_and_simple_checks_nothing() {
    let plans = plate_plans(&welded(), &library(), &Default::default()).unwrap();
    let u = plate_usage(&plans, &axial(), 1.0);
    let weld = u
        .iter()
        .find(|p| matches!(&p.part, PlatePart::Weld(_)))
        .unwrap();
    assert!(
        weld.usage.is_none() && weld.governs.contains("AWS D1.1"),
        "{weld:?}"
    );
    assert!(
        u.iter()
            .any(|p| p.checks.iter().any(|c| c.0.contains("4.4")))
    );
    let simple = format!("{}\n[analysis]\ncapacity = \"simple\"\n", welded());
    let plans = plate_plans(&simple, &library(), &Default::default()).unwrap();
    assert!(
        plate_usage(&plans, &axial(), 1.0)
            .iter()
            .all(|p| p.usage.is_none() && p.governs.contains("not checked"))
    );
}

/// A double cover splice (2026-11-05): each of L1's covers on its long
/// leg is checked as a plate for its share of L1's force by area — the
/// outer cover across the leg's 61 mm, the inner from the root's 15 mm,
/// 46 mm (both 8 mm thick): 61/107 and 46/107 of it; the short leg's
/// cover holds none of L1's bolts and is not checked for it.
#[test]
fn a_splices_covers_share_the_force_by_area() {
    use crate::joints_tests::SPLICED;
    let text = format!("{TOWER}{SPLICED}\n[analysis]\ncapacity = \"sia263-2013\"\n");
    let plans = plate_plans(&text, &library(), &Default::default()).unwrap();
    let mut axial = axial();
    axial.insert("L1".into(), 100e3);
    let u = plate_usage(&plans, &axial, 1.0);
    let l1: Vec<_> = u
        .iter()
        .filter(|p| p.part == PlatePart::Member("L1".into()))
        .collect();
    assert_eq!(l1.len(), 2, "{l1:?}");
    let outer = l1.iter().find(|p| p.plate.contains("outer")).unwrap();
    let inner = l1.iter().find(|p| p.plate.contains("inner")).unwrap();
    assert_eq!(outer.plate, "S1 (outer, long leg)");
    assert!(
        (outer.force - 100e3 * 61.0 / 107.0).abs() < 1.0,
        "{}",
        outer.force
    );
    assert!((outer.force + inner.force - 100e3).abs() < 1e-6);
    assert!(outer.usage.is_some_and(|x| x > 0.0 && x < 1.0), "{outer:?}");
    assert!(
        outer.checks.iter().any(|x| x.0.contains("(76)")),
        "{outer:?}"
    );
    assert!(
        outer
            .notes
            .iter()
            .any(|n| n.contains("shared between the covers by area")),
        "{:?}",
        outer.notes
    );
}

/// A folder with the plugin of `source` (under the repository's
/// `plugins/`) in its `Plugins/`, remembered as a place plugins are
/// looked for; none: the plugin is not built (a failure with
/// `OHL_REQUIRE_PLUGIN_TESTS` set).
fn with_plugin(source: &str, name: &str) -> Option<std::path::PathBuf> {
    let from = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../plugins")
        .join(source);
    let wasm = format!(
        "target/wasm32-wasip2/release/{}.wasm",
        name.replace('-', "_")
    );
    if !from.join(&wasm).is_file() {
        assert!(
            std::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
            "plugins/{source} not built"
        );
        eprintln!("skipping: plugins/{source} not built");
        return None;
    }
    let dir = std::env::temp_dir().join(format!("ohl-joint-check-{name}-{}", std::process::id()));
    let to = dir.join("Plugins").join(name);
    std::fs::create_dir_all(to.join(&wasm).parent().unwrap()).unwrap();
    std::fs::copy(from.join("ohl-plugin.toml"), to.join("ohl-plugin.toml")).unwrap();
    std::fs::copy(from.join(&wasm), to.join(&wasm)).unwrap();
    ohl_member_check::roots(Some(&dir));
    Some(dir)
}

/// The tests that call a plugin's joint check, one at a time: one counts
/// the calls.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn by_plugin(plugin: &str, method: &str) -> String {
    format!(
        "{}\n[analysis]\ncapacity = \"plugin:{plugin}/{method}\"\n",
        welded()
    )
}

/// The structure's member check a plugin's method (the example plugin,
/// through the real host; 2026-11-11): the plates' bolts and the weld are
/// checked by its joint check — one call for two load cases —, each row
/// with the plugin's words and what it says is not on the safe side; the
/// weld by the example's throat × length × f_y, and half the strength
/// factor doubles the usage. The bolt tools' minimum distances are the
/// plugin's (the example's 2, 4 and 2 diameters), a pair of bolts closer
/// than its spacing breaks it; the live resistance says where it comes
/// from. No value of ASCE 10-97 anywhere.
#[test]
fn a_plugins_joint_check_and_minimum_distances_are_used() {
    let _guard = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(dir) = with_plugin("examples/member-check-example", "member-check-example") else {
        return;
    };
    let text = by_plugin("member-check-example", "example-yield-euler");
    let plans = plate_plans(&text, &library(), &Default::default()).unwrap();
    assert!(plans.plugin.is_some());
    let forces = axial();
    let before = ohl_member_check::joints::joint_calls();
    let rows = crate::plate_usage_cases(&plans, &[("one", 1.0, &forces), ("half", 0.5, &forces)]);
    assert_eq!(
        ohl_member_check::joints::joint_calls() - before,
        1,
        "one call for both cases"
    );
    assert_eq!(rows.len(), 2);
    let weld = |n: usize| {
        rows[n]
            .iter()
            .find(|p| matches!(&p.part, PlatePart::Weld(_)))
            .unwrap()
    };
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let seam = d.joint("N2").unwrap().plates[0].welds[0].length();
    let w = weld(0);
    assert!((w.force - 40e3).abs() < 1e-6, "{w:?}");
    assert!(
        w.governs.starts_with("throat × length × f_y (example)")
            && w.governs.contains("[not on the safe side: EXAMPLE METHOD"),
        "{w:?}"
    );
    // The fixture's plates take the members' steel: its yield strength
    // from the usage, the same for both cases.
    let fy = 40e3 / (w.usage.unwrap() * 0.006 * seam);
    assert!(fy > 200e6 && fy < 500e6, "{fy}");
    assert!((weld(1).usage.unwrap() - 2.0 * w.usage.unwrap()).abs() < 1e-12);
    for row in rows.iter().flatten() {
        assert!(
            !row.governs.contains("ASCE") && row.checks.iter().all(|c| !c.0.contains("ASCE")),
            "{row:?}"
        );
        // Checked by the example, or said not checked by it.
        assert!(
            (row.usage.is_some() && row.governs.contains("(example)"))
                || row
                    .governs
                    .starts_with("NOT CHECKED by member-check-example, NOT ON THE SAFE SIDE"),
            "{row:?}"
        );
    }
    let bolts = rows[0]
        .iter()
        .find(|p| p.part == PlatePart::Member("D1".into()))
        .unwrap();
    assert!(bolts.usage.is_some_and(|u| u > 0.0), "{bolts:?}");
    // The bolt tools' rules: the plugin's.
    let r = crate::bolt_rules(&text, &library(), &Default::default(), "D1", "N2", None)
        .expect("the plugin's minimum distances");
    assert!(r.code.contains("member-check-example"), "{}", r.code);
    let dia = r.diameter;
    assert!((r.end - 2.0 * dia).abs() < 1e-12 && (r.spacing - 4.0 * dia).abs() < 1e-12);
    assert!((r.edge - 2.0 * dia).abs() < 1e-12);
    assert_eq!(r.clauses[1], "example: 4 × diameter");
    assert!(crate::no_bolt_rules(&text).is_none());
    let close = crate::breaches(&r, &[[0.1, 0.03], [0.1 + 3.0 * dia, 0.03]], 0.0);
    assert!(
        close
            .iter()
            .any(|b| b.what.contains("example: 4 × diameter")),
        "{close:?}"
    );
    let e = crate::member_capacity(&text, &library(), &Default::default(), "D1").unwrap_err();
    assert!(
        e.starts_with("resistance: from member-check-example after a run"),
        "{e}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// A plugin with the member check alone (no `joint-check`, no
/// `bolt-distances` export): every plate part and the weld read "NOT
/// CHECKED by the plugin, NOT ON THE SAFE SIDE" without a usage — no silent
/// ASCE 10-97 —, the bolt tools have no rules and say "no minimum
/// distances from the plugin". With a method of the core the same file is
/// checked as before.
#[test]
fn a_plugin_without_a_joint_check_leaves_the_joints_not_checked() {
    let _guard = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(dir) = with_plugin("tests/member-check-only", "member-check-only") else {
        return;
    };
    let text = by_plugin("member-check-only", "members-only");
    let plans = plate_plans(&text, &library(), &Default::default()).unwrap();
    let forces = axial();
    let rows = crate::plate_usage_cases(&plans, &[("one", 1.0, &forces)]);
    assert!(rows[0].len() >= 3, "{rows:?}");
    for row in &rows[0] {
        assert!(row.usage.is_none() && row.checks.is_empty(), "{row:?}");
        assert!(
            row.governs.starts_with(
                "NOT CHECKED by member-check-only, NOT ON THE SAFE SIDE: the plugin \
                 member-check-only has no joint check"
            ),
            "{row:?}"
        );
    }
    assert!(crate::bolt_rules(&text, &library(), &Default::default(), "D1", "N2", None).is_none());
    assert_eq!(
        crate::no_bolt_rules(&text).as_deref(),
        Some(
            "no minimum distances from member-check-only: the plugin has no bolt-distances export"
        )
    );
    std::fs::remove_dir_all(&dir).ok();
    // The core's own method: its rules, no such note.
    assert!(crate::no_bolt_rules(&welded()).is_none());
    assert!(
        crate::bolt_rules(&welded(), &library(), &Default::default(), "D1", "N2", None)
            .is_some_and(|r| r.code == "ASCE 10-97")
    );
}
