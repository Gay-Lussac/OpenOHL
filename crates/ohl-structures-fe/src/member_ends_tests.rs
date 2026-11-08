// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The check takes its conditions from the joints (stage 4.1 and 4.2 of
//! the joint details, 2026-11-02): each end's eccentricity and restraint
//! given, the member's, worked out or assumed; the connection's capacity
//! at each end, the weaker governing; a library bolt's strengths; a tower
//! without connections checked as before. Values by hand.

use ohl_sections::SectionLibrary;
use ohl_structures::bolts::BoltLibrary;

use crate::asce10::{self, effective_slenderness};
use crate::design::Ends;
use crate::member_ends::{Source, end_conditions};
use crate::model::Model;

/// A leg in two members (sections "L", 20 cm²) N1–N2–N3 and a diagonal
/// D1 (section "D", 10 cm², 6 mm) from N4 to N2: 5 m long, r_v 1.414 cm,
/// L/r 353.6 (above 120: the restraint curves). Steel f_y 355, f_u 510
/// MPa. D1's own design data: two bolts of 16 mm, 60 kN per plane, a
/// hole of 17.5 mm, one per section, the long leg bolted, 6 mm thick.
const TOWER: &str = r#"
[[material]]
id = "steel"
elasticity = "210 GPa"
yield = "355 MPa"
ultimate = "510 MPa"

[[section]]
id = "L"
material = "steel"
area = "20 cm2"
iy = "200 cm4"
iz = "200 cm4"
i_min = "80 cm4"

[[section]]
id = "D"
material = "steel"
area = "10 cm2"
iy = "50 cm4"
iz = "50 cm4"
i_min = "20 cm4"

[[node]]
id = "N1"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N2"
point = ["0 m", "0 m", "4 m"]

[[node]]
id = "N3"
point = ["0 m", "0 m", "8 m"]
support = "fixed"

[[node]]
id = "N4"
point = ["3 m", "0 m", "0 m"]
support = "fixed"

[[member]]
id = "L1"
nodes = ["N1", "N2"]
section = "L"
design = { class = "leg" }

[[member]]
id = "L2"
nodes = ["N2", "N3"]
section = "L"
design = { class = "leg" }

[[member]]
id = "D1"
nodes = ["N4", "N2"]
section = "D"
[member.design]
class = "bracing"
length_ratios = [1, 1, 1]
thickness = "6 mm"
connected_leg = "long"
bolts = { diameter = "16 mm", hole = "17.5 mm", count = 2, shear_capacity = "60 kN", holes = 1 }
"#;

/// D1's L/r: 5 m over r_v = √(20 cm⁴ / 10 cm²).
fn l_r() -> f64 {
    5.0 / (20e-8f64 / 10e-4).sqrt()
}

fn model(extra: &str, bolts: &BoltLibrary) -> Model {
    Model::from_structure_type_with_bolts(
        &format!("{TOWER}{extra}"),
        &SectionLibrary::default(),
        bolts,
        None,
    )
    .unwrap()
}

const D1: usize = 2;

/// Euler's column capacity of D1 at `kl_r`, N (KL/r above C_c).
fn column(kl_r: f64) -> f64 {
    std::f64::consts::PI.powi(2) * 210e9 / (kl_r * kl_r) * 10e-4
}

/// Two bolts into the leg at N2 only (its stiffness I/L five times D1's,
/// the only member it holds there; ASCE 10-97 C3.7.4 (b)): restrained
/// there, worked out; the end at N4 (no connection) not restrained,
/// assumed; eccentric at both (one leg bolted at N2, worked out; N4
/// assumed). KL/r by curve 5 (one end restrained): 28.6 + 0.762 L/r.
#[test]
fn a_connection_at_one_end_only() {
    let m = model(
        "[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nbolts = { count = 2, diameter = \"16 mm\" }\n",
        &BoltLibrary::default(),
    );
    let e = end_conditions(&m, D1);
    assert_eq!(e.connected, [false, true]);
    assert_eq!(e.restrained[1].value, Some(true));
    assert_eq!(e.restrained[1].source, Source::WorkedOut);
    assert_eq!(
        e.restrained[1].why.as_deref(),
        Some("2 bolts onto L2 (I/L 5.00 of those it holds)")
    );
    assert_eq!(e.restrained[0].source, Source::Assumed);
    assert_eq!(e.restrained[0].value, Some(false));
    assert_eq!(e.eccentric[1].source, Source::WorkedOut);
    assert_eq!(e.eccentric[0].source, Source::Assumed);
    assert_eq!(
        (e.eccentricity, e.restraint),
        (Ends::BothEnds, Ends::OneEnd)
    );
    let c = asce10::capacity(&m, D1, None);
    let kl_r = 28.6 + 0.762 * l_r();
    assert!((c.kl_r.unwrap() - kl_r).abs() < 1e-9, "{c:?}");
    assert!((c.compression.as_ref().unwrap().0 - column(kl_r)).abs() < 1e-6);
    // Said: what came from the joints, what was assumed (one end each).
    assert!(
        c.compression_assumed
            .contains(&"eccentric at N4".to_owned())
    );
    assert!(
        c.compression_assumed
            .contains(&"not restrained at N4".to_owned())
    );
    assert!(
        c.joints.iter().any(|j| j.starts_with(
            "N2 eccentric (worked out: one leg bolted), restrained (worked out: 2 bolts onto L2 (I/L 5.00 of those it holds))"
        )),
        "{:?}",
        c.joints
    );
    assert!(
        c.joints
            .iter()
            .any(|j| j.contains("N4") && j.contains("no connection given"))
    );
    // Without the connection: no restraint (assumed, both ends), L/r.
    let m = model("", &BoltLibrary::default());
    let c = asce10::capacity(&m, D1, None);
    assert!((c.kl_r.unwrap() - l_r()).abs() < 1e-9);
    assert!(
        c.compression_assumed
            .contains(&"no end restraint".to_owned())
    );
    assert!(c.joints.is_empty());
}

/// Given values win, then the member's flags; the member's "one end"
/// is the end the other end's given value leaves.
#[test]
fn given_then_the_members_then_worked_out() {
    let at_n2 = "[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nbolts = { count = 2, diameter = \"16 mm\" }\n";
    let m = model(
        &format!("{at_n2}eccentric = false\nrestrained = false\n"),
        &BoltLibrary::default(),
    );
    let e = end_conditions(&m, D1);
    assert_eq!(e.eccentric[1].source, Source::Given);
    assert_eq!((e.eccentricity, e.restraint), (Ends::OneEnd, Ends::None));
    // The member: one end restrained; N2 not (given), so N4.
    let text = format!("{TOWER}{at_n2}restrained = false\n").replace(
        "thickness = \"6 mm\"",
        "thickness = \"6 mm\"\nend_restraint = \"one_end\"",
    );
    let m = Model::from_structure_type_with_bolts(
        &text,
        &SectionLibrary::default(),
        &BoltLibrary::default(),
        None,
    )
    .unwrap();
    let e = end_conditions(&m, D1);
    assert_eq!(e.restrained[0].value, Some(true));
    assert_eq!(e.restrained[0].source, Source::Member);
    assert_eq!(e.restraint, Ends::OneEnd);
    let c = asce10::capacity(&m, D1, None);
    let kl_r = effective_slenderness(l_r(), Ends::BothEnds, Ends::OneEnd);
    assert!((c.kl_r.unwrap() - kl_r).abs() < 1e-9);
}

/// One bolt at N2, two at N4 (the member's): the bolt shear at N2, 60 kN,
/// governs the tension (N4: 120 kN; bearing 1.5 × 510 MPa × 16 mm × 6 mm
/// = 73.44 kN per bolt; the net section (10 cm² − (17.5 + 1.6) mm × 6 mm) ×
/// 355 MPa × 0.9 = 282.9 kN, holes punched), named by its end.
#[test]
fn the_weaker_end_governs() {
    let m = model(
        "[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nbolts = { count = 1, diameter = \"16 mm\" }\n",
        &BoltLibrary::default(),
    );
    let c = asce10::capacity(&m, D1, None);
    let (t, what) = c.tension.clone().unwrap();
    assert!((t - 60e3).abs() < 1e-6, "{t}");
    assert_eq!(what, "bolt shear at N2");
    // One bolt: not restrained at N2, worked out.
    let e = end_conditions(&m, D1);
    assert_eq!(e.restrained[1].why.as_deref(), Some("one bolt"));
    // Both ends with two bolts: the bearing of two, 146.9 kN, above the
    // shear: 120 kN, no end named (the ends alike).
    let m = model("", &BoltLibrary::default());
    let (t, what) = asce10::capacity(&m, D1, None).tension.unwrap();
    assert!((t - 120e3).abs() < 1e-6);
    assert_eq!(what, "bolt shear");
    // Two gauge lines at N2: two holes in its section, the net section
    // (10 cm² − 2 × (17.5 + 1.6) mm × 6 mm) × 355 MPa × 0.9 = 246.3 kN (the
    // holes punched, 3.10.1) — above the bolts, so the shear still governs;
    // the net section itself by hand.
    let net = (10e-4 - 2.0 * (0.0175 + 0.0016) * 0.006) * 355e6 * 0.9;
    let m = model(
        "[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nbolts = { count = 4, diameter = \"16 mm\" }\ngauge_lines = 2\n",
        &BoltLibrary::default(),
    );
    let c = asce10::capacity(&m, D1, None);
    assert_eq!(c.tension.as_ref().unwrap().1, "bolt shear at N4");
    let m2 = {
        let mut m2 = m.clone();
        m2.members[D1].design.bolts.as_mut().unwrap().shear_capacity = Some(1e6);
        m2.members[D1].design.ends[1]
            .as_mut()
            .unwrap()
            .bolts
            .as_mut()
            .unwrap()
            .count = 40;
        m2.members[D1].design.bolts.as_mut().unwrap().count = 40;
        m2
    };
    let (t, what) = asce10::capacity(&m2, D1, None).tension.unwrap();
    assert!((t - net).abs() < 1e-6, "{t} {what}");
    assert_eq!(what, "net section at N2");
}

/// A bolt of the library: its shear capacity per plane with the threads
/// out of the plane (89 kN for X17-example) when the connection says so,
/// else with the threads in (71 kN); the member's at the other end.
#[test]
fn a_library_bolt_gives_its_strength() {
    let lib = BoltLibrary::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../libraries/bolts/example-bolts.ohlb"),
    )
    .unwrap();
    let conn = |threads: &str| {
        format!(
            "[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nbolts = {{ count = 1, bolt = \"X17-example\"{threads} }}\n"
        )
    };
    let shank = lib
        .get("X17-example")
        .unwrap()
        .shear_capacity_shank
        .unwrap()
        .si();
    let threads = lib.get("X17-example").unwrap().shear_capacity.unwrap().si();
    let m = model(&conn(", threads_in_shear = false"), &lib);
    let c = asce10::capacity(&m, D1, None);
    let (t, what) = c.tension.clone().unwrap();
    // Bearing of one 17 mm bolt: 1.5 × 510 MPa × 17 mm × 6 mm = 78.03 kN.
    let bearing = 1.5 * 510e6 * 0.017 * 0.006;
    assert!((t - shank.min(bearing)).abs() < 1e-6, "{t} {what}");
    assert!(c.joints.iter().any(|j| j.contains("bolt X17-example")));
    let m = model(&conn(""), &lib);
    let (t, what) = asce10::capacity(&m, D1, None).tension.unwrap();
    assert!((t - threads.min(bearing)).abs() < 1e-6, "{t}");
    assert_eq!(what, "bolt shear at N2");
    // Not in the library: said; the member's bolts at that end.
    let m = model(&conn(""), &BoltLibrary::default());
    let c = asce10::capacity(&m, D1, None);
    assert!(c.joints.iter().any(|j| j.contains("in no bolt library")));
    assert_eq!(c.tension.unwrap().1, "bolt shear");
}

/// A tower without connections is checked exactly as before: a connection
/// leading nowhere changes nothing, and a connection that gives what the
/// member gives (its flags kept) the capacities and assumptions not
/// either (only the joints' note).
#[test]
fn without_connections_as_before() {
    let flags = |t: &str| {
        t.replace(
            "thickness = \"6 mm\"",
            "thickness = \"6 mm\"\nend_eccentricity = \"both_ends\"\nend_restraint = \"none\"",
        )
    };
    let lib = BoltLibrary::default();
    let base = Model::from_structure_type_with_bolts(
        &flags(TOWER),
        &SectionLibrary::default(),
        &lib,
        None,
    )
    .unwrap();
    let before = asce10::capacity(&base, D1, None);
    // By hand: KL/r = L/r (no restraint), Euler; tension the bolts' shear.
    assert!((before.kl_r.unwrap() - l_r()).abs() < 1e-9);
    assert!((before.compression.as_ref().unwrap().0 - column(l_r())).abs() < 1e-6);
    assert!(before.joints.is_empty());
    let nowhere = format!(
        "{}[[connection]]\nmember = \"X9\"\njoint = \"N2\"\nrestrained = true\n",
        flags(TOWER)
    );
    let m = Model::from_structure_type_with_bolts(&nowhere, &SectionLibrary::default(), &lib, None)
        .unwrap();
    assert_eq!(asce10::capacity(&m, D1, None), before);
    let same = format!(
        "{}[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nleg = \"long\"\n",
        flags(TOWER)
    );
    let m = Model::from_structure_type_with_bolts(&same, &SectionLibrary::default(), &lib, None)
        .unwrap();
    let mut after = asce10::capacity(&m, D1, None);
    assert!(!after.joints.is_empty());
    after.joints.clear();
    assert_eq!(after, before);
}

/// A structure file's bolts are looked up in `Bolts/` beside it when it
/// is read from its path.
#[test]
fn the_bolts_beside_the_file() {
    let dir = std::env::temp_dir().join(format!("ohl-bolts-beside-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Bolts")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../libraries/bolts/example-bolts.ohlb"),
        dir.join("Bolts/example-bolts.ohlb"),
    )
    .unwrap();
    let path = dir.join("t.ohls");
    std::fs::write(
        &path,
        format!(
            "{TOWER}[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nbolts = {{ count = 1, bolt = \"X17-example\" }}\n"
        ),
    )
    .unwrap();
    let m = Model::load_with(&path, &SectionLibrary::default()).unwrap();
    let e = m.members[D1].design.ends[1].as_ref().unwrap();
    assert!((e.bolts.unwrap().diameter.unwrap() - 0.017).abs() < 1e-12);
    assert!(e.notes.is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// ASCE 10-97 3.7.4.5 and C3.7.4: two bolts onto a gusset plate only do
/// not restrain the end; two bolts onto a member restrain it only when
/// its stiffness I/L is at least the sum of those it holds there (C3.7.4
/// (b)): a leg of I_min 80 cm⁴ over 4 m (20 cm³) holds D1 (20 cm⁴ / 5 m =
/// 4 cm³), but not a D1 of 120 cm⁴ (24 cm³).
#[test]
fn restraint_by_plate_and_by_stiffness() {
    let m = model(
        "[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         [[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"G1\"\n\
         bolts = { count = 3, diameter = \"16 mm\" }\n",
        &BoltLibrary::default(),
    );
    let e = end_conditions(&m, D1);
    assert_eq!(e.restrained[1].value, Some(false));
    assert_eq!(e.restrained[1].source, Source::WorkedOut);
    assert_eq!(
        e.restrained[1].why.as_deref(),
        Some("3 bolts onto plate G1 only")
    );
    let conn = "[[connection]]\nmember = \"D1\"\njoint = \"N2\"\n\
                bolts = { count = 2, diameter = \"16 mm\" }\n";
    let mut m = model(conn, &BoltLibrary::default());
    assert_eq!(end_conditions(&m, D1).restrained[1].value, Some(true));
    // Worked out by the I/L rule, which cannot know the case or the
    // plane: said, not on the safe side (2026-11-08; D1's L/r above 120).
    let c = asce10::capacity(&m, D1, None);
    assert!(
        c.compression_assumed
            .iter()
            .any(|n| n.starts_with("restraint at N2 by the I/L rule") && n.contains("not checked")),
        "{:?}",
        c.compression_assumed
    );
    let d = m.members[D1].section;
    m.sections[d].i_min = 120e-8;
    let e = end_conditions(&m, D1);
    assert_eq!(e.restrained[1].value, Some(false));
    assert_eq!(
        e.restrained[1].why.as_deref(),
        Some("2 bolts onto L2 (I/L 0.83 of those it holds)")
    );
}

/// A splice at N2 with covers on both faces of the long leg: two shear
/// planes per bolt, worked out (over the member's design data); one cover:
/// one; given on the bolts: as given, the difference said.
#[test]
fn a_splice_gives_its_shear_planes() {
    use crate::design::Onto;
    let splice = |covers: &str, planes: &str| {
        format!(
            "[[splice]]\nid = \"S1\"\njoint = \"N2\"\nmembers = [\"D1\", \"L1\"]\n{covers}\
             [[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"S1\"\nleg = \"long\"\n\
             bolts = {{ count = 2, diameter = \"16 mm\"{planes} }}\n"
        )
    };
    let outer = "[[splice.cover]]\nface = \"outer\"\nleg = \"long\"\nthickness = \"8 mm\"\n";
    let inner = "[[splice.cover]]\nface = \"inner\"\nleg = \"long\"\nthickness = \"6 mm\"\n";
    let planes = |m: &Model| {
        let d = &m.members[D1].design;
        let mut notes = Vec::new();
        let b = asce10::end_bolts(d, d.ends[1].as_ref(), &mut notes)
            .unwrap()
            .0;
        (b.shear_planes, notes)
    };
    let m = model(
        &splice(&format!("{outer}{inner}"), ""),
        &BoltLibrary::default(),
    );
    let end = m.members[D1].design.ends[1].as_ref().unwrap();
    match end.onto.as_ref().unwrap() {
        Onto::Splice { planes, covers, .. } => {
            assert_eq!(*planes, 2);
            assert!((covers - 0.014).abs() < 1e-12);
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(planes(&m).0, 2);
    let m = model(&splice(outer, ""), &BoltLibrary::default());
    assert_eq!(planes(&m).0, 1);
    let m = model(
        &splice(&format!("{outer}{inner}"), ", shear_planes = 1"),
        &BoltLibrary::default(),
    );
    let (n, notes) = planes(&m);
    assert_eq!(n, 1);
    assert!(
        notes
            .iter()
            .any(|x| x == "shear planes: 1 given (2 worked out: 2 covers on the leg)"),
        "{notes:?}"
    );
}

/// L1 runs on into L2 through N2 (given on L1 only): both ends are one
/// part — concentric, worked out; the restraint not (assumed: none, on
/// the safe side); no connection checked there;
/// the diagonal given continuous onto the leg (not in line) computes as
/// bolted, said.
#[test]
fn a_continuous_leg_has_no_connection_at_the_joint() {
    let m = model(
        "[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"L2\"\n\
         [[connection]]\nmember = \"D1\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"L1\"\n",
        &BoltLibrary::default(),
    );
    let (l1, l2) = (0, 1);
    assert_eq!(
        m.members[l1].design.ends[1].as_ref().unwrap().continuous,
        Some(l2)
    );
    assert_eq!(
        m.members[l2].design.ends[0].as_ref().unwrap().continuous,
        Some(l1)
    );
    let e = end_conditions(&m, l2);
    assert_eq!(e.eccentric[0].value, Some(false));
    assert_eq!(e.eccentric[0].why.as_deref(), Some("continuous into L1"));
    assert_eq!(e.eccentric[0].source, Source::WorkedOut);
    assert_ne!(e.restrained[0].source, Source::WorkedOut);
    let mut notes = Vec::new();
    let d = &m.members[l1].design;
    assert!(asce10::end_bolts(d, d.ends[1].as_ref(), &mut notes).is_none());
    assert_eq!(
        notes,
        ["continuous through the joint: no connection to check"]
    );
    let d1 = m.members[D1].design.ends[1].as_ref().unwrap();
    assert_eq!(d1.kind, None);
    assert!(d1.continuous.is_none());
    assert!(
        d1.notes
            .iter()
            .any(|n| n.starts_with("continuous: not one part (D1 and L1 turn")),
        "{:?}",
        d1.notes
    );
}

/// D1 bolted onto L1 at N2, L1's leg 4 mm (thinner than D1's 6 mm): the
/// bearing on L1's leg governs, 2 × 1.5 × 510 MPa × 16 mm × 4 mm = 97.92
/// kN (ASCE 10-97 4.4), below the bolts' 120 kN and D1's own 146.88 kN.
#[test]
fn the_bolts_bear_on_the_thinner_leg_they_bolt_onto() {
    let mut m = model(
        "[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"L1\"\n\
         bolts = { count = 2, diameter = \"16 mm\" }\n",
        &BoltLibrary::default(),
    );
    m.members[0].design.thickness = Some(0.004);
    let c = asce10::capacity(&m, D1, None);
    let (t, what) = c.tension.clone().unwrap();
    assert!((t - 2.0 * 1.5 * 510e6 * 0.016 * 0.004).abs() < 1e-6, "{t}");
    assert_eq!(what, "bearing at N2");
    // L1 as thick as D1: D1's own bearing, as before.
    m.members[0].design.thickness = Some(0.006);
    let (_, what) = asce10::capacity(&m, D1, None).tension.unwrap();
    assert_eq!(what, "bolt shear");
}

/// Both legs bolted (2026-11-05): each gauge line has a hole on each leg,
/// so one gauge line makes two holes in the section; bolt places count
/// the lines across each leg.
#[test]
fn both_legs_bolted_hole_each_leg() {
    let holes = |extra: &str| {
        let m = model(
            &format!(
                "[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nleg = \"both\"\n\
                 bolts = {{ count = 4, diameter = \"16 mm\" }}\n{extra}"
            ),
            &BoltLibrary::default(),
        );
        m.members[D1].design.ends[1].as_ref().unwrap().holes
    };
    assert_eq!(holes("gauge_lines = 1\n"), Some(2));
    assert_eq!(
        holes(
            "bolt_points = [[\"30 mm\", \"30 mm\"], [\"80 mm\", \"30 mm\"]]\n\
             other_leg_points = [[\"30 mm\", \"25 mm\"], [\"80 mm\", \"40 mm\"]]\n"
        ),
        Some(3)
    );
}
