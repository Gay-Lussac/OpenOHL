// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The member wind's force law (owner 2026-11-15): the inclination factor
//! against SIA 261:2014 Tableau 74 (p. 112) at its angles and between
//! them, held below θ = 30°, round against sharp-edged shapes, a curve
//! given as data, the comparison law sin²θ reproducing the member wind
//! before 2026-11-15 bit for bit, the built-in force coefficients
//! (Tableaux 71 and 72), and a full tower (the synthetic example's
//! type-02 model with invented angle profiles).

use ohl_core::units::Angle;
use ohl_sections::{MemberLaw, SectionLibrary};

use crate::model::{Model, NodalLoad};
use crate::wind::{DragFrom, MemberWindSetup};
use crate::wind_law::{
    self, InclinationFrom, TAB74_BETA, TAB74_ROUND_HIGH, TAB74_ROUND_LOW, TAB74_SHARP,
    beta_degrees, code_drag, inclination, round_factor, sharp_factor,
};

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs().max(1e-12)
}

/// sin θ of β (degrees).
fn sin_of(beta: f64) -> f64 {
    beta.to_radians().cos()
}

/// Invented profiles (dimensions made up for the tests).
const LIBRARY: &str = r#"
[[material]]
id = "steel"
elasticity = "210 GPa"
density = "7850 kg/m3"

[[section]]
id = "L80"
shape = "equal_angle"
material = "steel"
h = "80 mm"
t = "8 mm"

[[section]]
id = "B50"
shape = "round_bar"
material = "steel"
d = "50 mm"

[[section]]
id = "G"
shape = "general"
material = "steel"
h = "100 mm"
b = "50 mm"
area = "1000 mm2"
iy = "100 cm4"
iz = "50 cm4"
"#;

/// One member of `section` from (0,0,0) to `end` (m), wind along +x.
fn one(section: &str, end: [f64; 3], analysis: &str) -> Model {
    let lib = SectionLibrary::parse(LIBRARY).unwrap();
    let text = format!(
        "[analysis]\nnonlinear = false\nself_weight = false\n{analysis}\n\
         [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"fixed\"\n\
         [[node]]\nid = \"B\"\npoint = [{}, {}, {}]\nsupport = \"fixed\"\n\
         [[member]]\nid = \"m\"\nnodes = [\"A\", \"B\"]\nsection = \"{section}\"\n",
        end[0], end[1], end[2]
    );
    Model::parse_with(&text, &lib).unwrap()
}

#[test]
fn the_sharp_edged_factor_is_tableau_74() {
    // At the table's angles: its values (read from SIA 261:2014 p. 112).
    let table = [1.0, 0.98, 0.93, 0.88, 0.8];
    for (b, f) in TAB74_BETA.iter().zip(table) {
        assert!(close(sharp_factor(*b), f, 1e-12), "β {b}");
        let k = inclination("equal_angle", sin_of(*b), None);
        assert!(close(k.k, f, 1e-9), "β {b}: {}", k.k);
        assert_eq!(k.from, InclinationFrom::Sharp);
        assert!(!k.held);
    }
    assert_eq!(TAB74_SHARP, table);
    // Linear between them, at least cos β (near 0° the straight line from
    // 1.0 to 0.98 lies below it).
    assert!(close(sharp_factor(7.5), 7.5f64.to_radians().cos(), 1e-12));
    assert!(close(sharp_factor(10.0), 1.0 - 0.02 * 10.0 / 15.0, 1e-12));
    assert!(close(sharp_factor(22.5), 0.955, 1e-12));
    assert!(close(sharp_factor(37.5), 0.905, 1e-12));
    assert!(close(sharp_factor(52.5), 0.84, 1e-12));
    // β from sin θ: θ = 45° is β = 45°.
    assert!(close(beta_degrees(0.5f64.sqrt()), 45.0, 1e-9));
    // Every shape that is not round takes it, even a general section.
    for shape in [
        "unequal_angle",
        "channel",
        "i_section",
        "general",
        "double_angle",
    ] {
        let k = inclination(shape, sin_of(45.0), None);
        assert_eq!(k.from, InclinationFrom::Sharp, "{shape}");
        assert!(close(k.k, 0.88, 1e-9));
    }
}

/// Below θ = 30° (β above 60°) SIA 261 gives nothing: the 60° factor is
/// held, said.
#[test]
fn below_30_degrees_the_factor_is_held() {
    for beta in [60.5, 70.0, 80.0, 89.0] {
        let k = inclination("equal_angle", sin_of(beta), None);
        assert!(close(k.k, 0.8, 1e-12) && k.held, "β {beta}: {k:?}");
        let r = inclination("circular_hollow", sin_of(beta), None);
        assert!(close(r.k, 0.5, 1e-9) && r.held, "β {beta}: {r:?}");
    }
    assert!(!inclination("equal_angle", sin_of(59.0), None).held);
}

/// Round bars: sin θ (= cos β), raised to Tableau 74's own ratio c_β /
/// c_β(0) where that is larger (the rows read from p. 112).
#[test]
fn round_bars_take_sin_theta() {
    assert_eq!(TAB74_ROUND_LOW, [1.2, 1.16, 1.04, 0.85, 0.6]);
    assert_eq!(TAB74_ROUND_HIGH, [0.6, 0.58, 0.53, 0.42, 0.28]);
    // At the table's angles: the largest of cos β and the two ratios.
    let expect = [1.0, 1.16 / 1.2, 0.53 / 0.6, 0.85 / 1.2, 0.5];
    for (b, f) in TAB74_BETA.iter().zip(expect) {
        let k = inclination("round_bar", sin_of(*b), None);
        assert_eq!(k.from, InclinationFrom::Round);
        assert!(close(k.k, f, 1e-9), "β {b}: {} vs {f}", k.k);
        // Never below sin θ, at most 2 % above (the >0.07 row at 30°).
        assert!(
            k.k >= sin_of(*b) - 1e-12 && k.k <= 1.02 * sin_of(*b),
            "β {b}"
        );
    }
    // Between the angles: never below cos β.
    for b in [5.0, 20.0, 40.0, 55.0] {
        assert!(round_factor(b) >= sin_of(b) - 1e-12);
    }
    // A round member against an angle at 45°: 0.708 against 0.88.
    assert!(
        inclination("circular_hollow", sin_of(45.0), None).k
            < inclination("equal_angle", sin_of(45.0), None).k
    );
}

/// A curve given in the section library's `[wind] inclination` replaces
/// the built-in one for its shape; `member_law = "sin2"` is the
/// cross-flow law.
#[test]
fn a_curve_given_as_data_and_the_comparison_law() {
    let lib = SectionLibrary::parse(
        "version = 2\n[wind]\nsource = \"invented\"\n\
         [wind.inclination.equal_angle]\nangle = [0, 40]\nfactor = [1.0, 0.6]\n",
    )
    .unwrap();
    let w = lib.wind.as_ref().unwrap();
    assert!(w.needs_version_2());
    let k = inclination("equal_angle", sin_of(20.0), Some(w));
    assert_eq!(k.from, InclinationFrom::Data);
    assert!(close(k.k, 0.8, 1e-9));
    let held = inclination("equal_angle", sin_of(50.0), Some(w));
    assert!(close(held.k, 0.6, 1e-12) && held.held);
    // Other shapes: the built-in law.
    assert_eq!(
        inclination("round_bar", sin_of(20.0), Some(w)).from,
        InclinationFrom::Round
    );
    let sin2 = SectionLibrary::parse("version = 2\n[wind]\nmember_law = \"sin2\"\n").unwrap();
    let w = sin2.wind.as_ref().unwrap();
    assert_eq!(w.member_law, MemberLaw::Sin2);
    let s = sin_of(30.0);
    let k = inclination("equal_angle", s, Some(w));
    assert_eq!(k.from, InclinationFrom::Sin2);
    assert!(close(k.k, s * s, 1e-15));
    // Mistakes refused.
    for (text, says) in [
        (
            "[wind.inclination.angle]\nangle = [0, 30]\nfactor = [1, 0.9]",
            "not a shape",
        ),
        (
            "[wind.inclination.round_bar]\nangle = [10, 30]\nfactor = [1, 0.9]",
            "starts at 0",
        ),
        (
            "[wind.inclination.round_bar]\nangle = [0, 30]\nfactor = [1]",
            "one factor per angle",
        ),
        (
            "[wind.inclination.round_bar]\nangle = [0, 95]\nfactor = [1, 0.5]",
            "within 90",
        ),
        ("[wind]\nmember_law = \"sin\"", "member_law"),
    ] {
        let e = SectionLibrary::parse(text).unwrap_err();
        assert!(e.contains(says), "{text}: {e}");
    }
}

/// The built-in force coefficients (Tableau 71 p. 109, Tableau 72 p. 110)
/// per shape of the example section library, on their reference widths.
#[test]
fn the_built_in_force_coefficients() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lib =
        SectionLibrary::load(&root.join("libraries/sections/example-sections.ohlsec")).unwrap();
    let get = |id: &str| code_drag(lib.section(id).unwrap());
    let check = |id: &str, c: f64, h: f64| {
        let d = get(id).unwrap_or_else(|| panic!("{id}: none"));
        assert!(close(d.coefficient, c, 1e-12), "{id}: {}", d.coefficient);
        assert!(close(d.reference, h, 1e-9), "{id}: {} vs {h}", d.reference);
    };
    check("EA-61x7", 2.1, 0.061);
    check("UA-83x52x6", 2.1, 0.083);
    check("CH-157", 2.05, 0.157);
    check("IS-213", 2.05, 0.213);
    check("RHS-121x79x5", 2.1, 0.121);
    check("CHS-167x7", 1.2, 0.167);
    check("RB-23", 1.2, 0.023);
    check(
        "POLE-12-430",
        2.1,
        0.430 / (std::f64::consts::PI / 12.0).cos(),
    );
    check("2S-61x7", 1.75, 2.0 * 0.061 + 0.011);
    check("4X-61x7", 1.75, 2.0 * 0.061 + 0.011);
    assert!(get("GEN-1").is_none(), "a general section has no shape");
    // Two angles back to back: the T of Tableau 71 when the back legs are
    // at most half the width, else the T with the long plate.
    let two = lib.section("2L-83x52x6").unwrap();
    let z = two.wind_width(Angle::from_degrees(0.0));
    let y = two.wind_width(Angle::from_degrees(90.0));
    let c = get("2L-83x52x6").unwrap();
    assert_eq!(
        c.coefficient,
        if z <= 0.5 * y {
            wind_law::TAB71_DOUBLE_ANGLE
        } else {
            wind_law::TAB71_DOUBLE_ANGLE_LONG
        }
    );
    assert!(close(c.reference, z.max(y), 1e-12));
}

/// One member: the force normal to it is q·c·h·L·k with the built-in
/// coefficient (c·h whatever its width across the wind), q·c·d·L·k with a
/// drag given; along the unit normal in the plane of it and the wind.
#[test]
fn one_member_takes_the_law_and_the_coefficient() {
    let q = 1000.0;
    // Vertical, normal to the wind.
    let m = one("L80", [0.0, 0.0, 3.0], "");
    let w = MemberWindSetup::new(&m, None).wind(q, [1.0, 0.0, 0.0]);
    let r = &w.members[0];
    assert_eq!(r.drag_from, DragFrom::Code);
    assert!(close(r.force, q * 2.1 * 0.08 * 3.0, 1e-12), "{}", r.force);
    assert!(close(w.total()[0], r.force, 1e-12));
    assert!(close(r.inclination, 1.0, 1e-12));
    assert!(
        w.notes
            .iter()
            .any(|n| n.contains("SIA 261:2014") && n.contains("equal_angle (1): 2.1")),
        "{:?}",
        w.notes
    );
    assert!(!w.notes.iter().any(|n| n.contains("take the drag factor 1")));
    // At 45° to the wind: × 0.88 normal to the member, in the x–z plane.
    let m = one("L80", [3.0, 0.0, 3.0], "");
    let w = MemberWindSetup::new(&m, None).wind(q, [1.0, 0.0, 0.0]);
    let r = &w.members[0];
    let len = 18f64.sqrt();
    assert!(close(r.force, q * 2.1 * 0.08 * len * 0.88, 1e-12));
    let t = w.total();
    let s = 0.5f64.sqrt();
    assert!(close(t[0], r.force * s, 1e-12) && close(t[2], -r.force * s, 1e-12));
    assert!(t[1].abs() < 1e-9);
    // With [analysis] drag given: on the projected width, no coefficient.
    let m = one("L80", [3.0, 0.0, 3.0], "drag = 1.8");
    let w = MemberWindSetup::new(&m, None).wind(q, [1.0, 0.0, 0.0]);
    let r = &w.members[0];
    let (_, _, d) = m.across(&m.members[0], [1.0, 0.0, 0.0]).unwrap();
    assert_eq!(r.drag_from, DragFrom::Generic);
    assert!(close(r.force, q * 1.8 * d * len * 0.88, 1e-12));
    // A round bar at 45°: 1.2 (Tableau 72) × 0.85/1.2.
    let m = one("B50", [3.0, 0.0, 3.0], "");
    let w = MemberWindSetup::new(&m, None).wind(q, [1.0, 0.0, 0.0]);
    assert!(close(
        w.members[0].force,
        q * 1.2 * 0.05 * len * 0.85 / 1.2,
        1e-12
    ));
    // A general section: the generic drag 1, said not on the safe side.
    let m = one("G", [0.0, 0.0, 3.0], "");
    let w = MemberWindSetup::new(&m, None).wind(q, [1.0, 0.0, 0.0]);
    assert_eq!(w.members[0].drag_from, DragFrom::Generic);
    assert!(
        w.notes
            .iter()
            .any(|n| n.starts_with("NOT ON THE SAFE SIDE: 1 member(s) take the drag factor 1")),
        "{:?}",
        w.notes
    );
}

/// The member wind before 2026-11-15, per member q·c·η·d·L·sin θ along
/// the normal of length sin θ, written out here.
fn old_law(m: &Model, q: f64, w: [f64; 3]) -> Vec<NodalLoad> {
    let mut out = Vec::new();
    for mem in &m.members {
        let Some((normal, sin, width)) = m.across(mem, w) else {
            continue;
        };
        let f = q * m.analysis.drag * 1.0 * width * m.length(mem) * sin;
        for node in [mem.i, mem.j] {
            out.push(NodalLoad {
                node,
                force: normal.map(|c| 0.5 * f * c),
                moment: [0.0; 3],
            });
        }
    }
    out
}

/// The type-02 model of the synthetic example (178 members), its sections
/// replaced by invented angles; `drag`: its `[analysis] drag` kept (1.8,
/// invented in the example) or left out.
fn tower(drag: bool) -> Model {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(
        root.join("examples/synthetic-line/Structures/type-02-model.ohlfe"),
    )
    .unwrap();
    let text = if drag {
        text
    } else {
        text.replace("\ndrag = 1.8\n", "\n")
    };
    let lib = SectionLibrary::parse(
        r#"
[[material]]
id = "steel"
elasticity = "210 GPa"
density = "7850 kg/m3"
[[section]]
id = "leg"
shape = "equal_angle"
material = "steel"
h = "120 mm"
t = "12 mm"
[[section]]
id = "brace"
shape = "equal_angle"
material = "steel"
h = "60 mm"
t = "6 mm"
[[section]]
id = "arm"
shape = "equal_angle"
material = "steel"
h = "90 mm"
t = "8 mm"
"#,
    )
    .unwrap();
    Model::parse_with(&text, &lib).unwrap()
}

/// `member_law = "sin2"` reproduces the member wind before 2026-11-15 bit
/// for bit (with a drag given, as the old default 1 is now a built-in
/// coefficient).
#[test]
fn the_sin2_law_reproduces_the_old_member_wind() {
    let lib = SectionLibrary::parse("version = 2\n[wind]\nmember_law = \"sin2\"\n").unwrap();
    let m = tower(true);
    for dir in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.6, 0.8, 0.0]] {
        let w = MemberWindSetup::new(&m, lib.wind.as_ref()).wind(1000.0, dir);
        let old = old_law(&m, 1000.0, dir);
        let members = w.members.len() * 2;
        assert_eq!(&w.loads[..members], &old[..], "direction {dir:?}");
        assert!(
            w.notes
                .iter()
                .any(|n| n.starts_with("NOT ON THE SAFE SIDE: member wind by the cross-flow")),
            "{:?}",
            w.notes
        );
    }
}

/// The full tower: every member's force at least the old one, the total
/// risen; the built-in coefficient 2.1 on the leg in place of the drag
/// 1.8 on the projected width.
#[test]
fn the_tower_member_wind_rises() {
    let sin2 = SectionLibrary::parse("version = 2\n[wind]\nmember_law = \"sin2\"\n").unwrap();
    let m = tower(true);
    for dir in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        let new = MemberWindSetup::new(&m, None).wind(1000.0, dir);
        let old = MemberWindSetup::new(&m, sin2.wind.as_ref()).wind(1000.0, dir);
        assert_eq!(new.members.len(), old.members.len());
        for (a, b) in new.members.iter().zip(&old.members) {
            assert!(
                a.force >= b.force * (1.0 - 1e-12),
                "{}",
                m.members[a.member].id
            );
        }
        let along = |w: &crate::wind::MemberWind| {
            let t = w.total();
            t[0] * dir[0] + t[1] * dir[1]
        };
        let rise = along(&new) / along(&old);
        // The inclined bracing gains most (c_red against sin²θ); the legs
        // and the members normal to the wind not at all.
        assert!(rise > 1.03 && rise < 1.35, "{dir:?}: {rise}");
        println!(
            "type-02, wind {dir:?}: sin² {:.1} N, inclination {:.1} N, ×{rise:.3}",
            along(&old),
            along(&new)
        );
    }
    // No drag given: 2.1 × h in place of 1.8 × d.
    let plain = tower(false);
    let dir = [1.0, 0.0, 0.0];
    let given = MemberWindSetup::new(&m, None).wind(1000.0, dir);
    let code = MemberWindSetup::new(&plain, None).wind(1000.0, dir);
    for (a, b) in code.members.iter().zip(&given.members) {
        assert_eq!(a.drag_from, DragFrom::Code);
        let mem = &plain.members[a.member];
        let (_, _, d) = plain.across(mem, dir).unwrap();
        let h = plain.sections[mem.section]
            .profile
            .as_ref()
            .unwrap()
            .wind_width(Angle::from_degrees(0.0));
        assert!(
            close(a.force / b.force, 2.1 * h / (1.8 * d), 1e-9),
            "{}",
            mem.id
        );
    }
    println!(
        "type-02, wind along x: drag 1.8 given {:.1} N, built-in 2.1 {:.1} N",
        given.total()[0],
        code.total()[0]
    );
}
