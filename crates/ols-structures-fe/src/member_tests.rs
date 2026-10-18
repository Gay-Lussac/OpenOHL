// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A member's own capacities, effective length and angle orientation, on
//! invented models.

use ols_structures::LibraryItem;

use crate::{Loads, Model, Options, default_budget, member_usage, solve};

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs().max(1e-30)
}

const MATERIAL: &str = "[analysis]\nnonlinear = false\n\
    [[material]]\nid = \"steel\"\nelasticity = \"210 GPa\"\nyield = \"355 MPa\"\n";

/// A bar from a pinned joint to a joint held along y and z, pulled or
/// pushed along x by `force` (N), its member written with `extra`.
fn bar(extra: &str) -> String {
    format!(
        "{MATERIAL}\
         [[section]]\nid = \"s\"\nmaterial = \"steel\"\narea = \"10 cm2\"\ni_min = \"20 cm4\"\n\
         [[node]]\nid = \"a\"\npoint = [\"0 m\", \"0 m\", \"0 m\"]\nsupport = \"pinned\"\n\
         [[node]]\nid = \"b\"\npoint = [\"3 m\", \"0 m\", \"0 m\"]\nsupport = [\"y\", \"z\"]\n\
         [[member]]\nid = \"m\"\nnodes = [\"a\", \"b\"]\nsection = \"s\"\n{extra}"
    )
}

fn usage(text: &str, force: f64) -> (f64, String) {
    let m = Model::parse(text).unwrap_or_else(|e| panic!("{e}"));
    let loads = Loads {
        nodal: vec![crate::NodalLoad {
            node: 1,
            force: [force, 0.0, 0.0],
            moment: [0.0; 3],
        }],
        self_weight: 0.0,
    };
    let o = Options {
        nonlinear: false,
        ..Options::default()
    };
    let s = solve(&m, &loads, &o, &default_budget()).unwrap();
    let u = member_usage(&m, &s, 0.9);
    (u[0].usage.unwrap(), u[0].governs.clone())
}

/// `capacity_tension` and `capacity_compression` replace the computed
/// capacities (a given compression the buckling too); `lk` replaces
/// k × length in the Euler load; absent, the computation is as before.
#[test]
fn member_capacities_and_effective_length_override_the_computed_ones() {
    let a = 10e-4;
    let fy = 355e6;
    let i = 20e-8;
    let e = 210e9;
    // Tension: area × yield, or the member's.
    let (u, _) = usage(&bar(""), 100e3);
    assert!(close(u, 100e3 / (0.9 * a * fy), 1e-12));
    let (u, g) = usage(&bar("capacity_tension = \"180 kN\"\n"), 100e3);
    assert!(close(u, 100e3 / (0.9 * 180e3), 1e-12), "{u}");
    assert!(g.ends_with("of 180.0 kN (given)"), "{g}");
    // Compression: the Euler load of k × L, of lk, or the member's.
    let euler = |kl: f64| std::f64::consts::PI.powi(2) * e * i / (kl * kl);
    let (u, g) = usage(&bar(""), -30e3);
    assert!(close(u, 30e3 / (0.9 * euler(3.0)), 1e-12), "{u}");
    assert!(g.contains("(buckling)"), "{g}");
    let (u, _) = usage(&bar("k = 0.8\n"), -30e3);
    assert!(close(u, 30e3 / (0.9 * euler(2.4)), 1e-12));
    let (u, _) = usage(&bar("lk = \"1.5 m\"\n"), -30e3);
    assert!(close(u, 30e3 / (0.9 * euler(1.5)), 1e-12));
    let (u, g) = usage(&bar("capacity_compression = \"95 kN\"\n"), -30e3);
    assert!(close(u, 30e3 / (0.9 * 95e3), 1e-12), "{u}");
    assert!(g.ends_with("of 95.0 kN (given)"), "{g}");
    // The tension capacity leaves compression alone, and the other way.
    let (u, _) = usage(&bar("capacity_tension = \"180 kN\"\n"), -30e3);
    assert!(close(u, 30e3 / (0.9 * euler(3.0)), 1e-12));
    // Units, and only valid values or known keys.
    let (u, _) = usage(&bar("capacity_tension = \"40.5 kip\"\n"), 100e3);
    assert!(
        close(u, 100e3 / (0.9 * 40.5 * 4448.2216152605), 1e-9),
        "{u}"
    );
    for bad in [
        "capacity_tension = \"0 kN\"\n",
        "capacity_compression = \"-5 kN\"\n",
        "lk = \"0 m\"\n",
        "capacity_tensoin = \"5 kN\"\n",
        "lk = \"3 kN\"\n",
    ] {
        assert!(Model::parse(&bar(bad)).is_err(), "{bad}");
    }
}

/// The member fields in a structure type file (`.olss`): read by the
/// structure, given to the model.
#[test]
fn member_fields_in_a_structure_file() {
    let text = format!(
        "id = \"t\"\nname = \"t\"\nkind = \"lattice\"\n{}",
        bar(
            "lk = \"2 m\"\ncapacity_tension = \"150 kN\"\ncapacity_compression = \"60 kN\"\n\
             orientation = \"y+z+\"\ngroup = \"g\"\n"
        )
    );
    let t = ols_structures::StructureType::parse(std::path::Path::new("t.olss"), &text);
    assert!(t.is_ok(), "{:?}", t.err());
    let m = Model::from_structure_type(&text).unwrap();
    let b = &m.members[0];
    assert_eq!(
        (b.lk, b.capacity_tension, b.capacity_compression),
        (Some(2.0), Some(150e3), Some(60e3))
    );
    assert_eq!(b.reference, Some([0.0, 1.0, 0.0]));
}

/// A frame cantilever along x of an unequal angle (iy = 100 cm4, iz = 400
/// cm4), loaded down at its tip: with its legs towards +y and +z the
/// local z is up and the bending about the local y (iy); towards +z and
/// −y the local y is up and the bending about z (iz). The tip deflection
/// is PL³/3EI of the axis the orientation gives.
#[test]
fn orientation_turns_the_bending_axis() {
    let model = |orientation: &str, iz: &str| {
        format!(
            "{MATERIAL}\
             [[section]]\nid = \"L\"\nmaterial = \"steel\"\narea = \"20 cm2\"\n\
             iy = \"100 cm4\"\niz = \"{iz}\"\nwy = \"20 cm3\"\nwz = \"40 cm3\"\n\
             [[node]]\nid = \"a\"\npoint = [\"0 m\", \"0 m\", \"0 m\"]\nsupport = \"fixed\"\n\
             [[node]]\nid = \"b\"\npoint = [\"2 m\", \"0 m\", \"0 m\"]\n\
             [[member]]\nid = \"m\"\nnodes = [\"a\", \"b\"]\nsection = \"L\"\nkind = \"frame\"\n\
             orientation = \"{orientation}\"\n"
        )
    };
    let tip = |text: &str| {
        let m = Model::parse(text).unwrap_or_else(|e| panic!("{e}"));
        let loads = Loads {
            nodal: vec![crate::NodalLoad {
                node: 1,
                force: [0.0, 0.0, -1e3],
                moment: [0.0; 3],
            }],
            self_weight: 0.0,
        };
        let o = Options {
            nonlinear: false,
            ..Options::default()
        };
        solve(&m, &loads, &o, &default_budget())
            .unwrap()
            .displacements[1][2]
    };
    let pl3 = 1e3 * 8.0 / (3.0 * 210e9);
    let w = tip(&model("y+z+", "400 cm4"));
    assert!(close(w, -pl3 / 100e-8, 1e-9), "{w}");
    let w = tip(&model("z+y-", "400 cm4"));
    assert!(close(w, -pl3 / 400e-8, 1e-9), "{w}");
    // The default (reference global z: local y up) is the second.
    let plain = model("y+z+", "400 cm4").replace("orientation = \"y+z+\"\n", "");
    assert!(close(tip(&plain), -pl3 / 400e-8, 1e-9));
    // The mirror image of an unequal angle is refused, naming the fix;
    // an equal angle's legs are swapped.
    let e = Model::parse(&model("z+y+", "400 cm4")).unwrap_err();
    assert!(e.contains("give \"y+z+\""), "{e}");
    let equal = model("z+y+", "100 cm4").replace("wz = \"40 cm3\"", "wz = \"20 cm3\"");
    let m = Model::parse(&equal).unwrap();
    assert_eq!(m.members[0].reference, Some([0.0, 1.0, 0.0]));
    // A direction along the member, both along one line, or with a
    // reference too: refused.
    for (o, why) in [
        ("x+y+", "along the member"),
        ("y+y-", "parallel"),
        ("q+y+", "two directions"),
    ] {
        let e = Model::parse(&model(o, "400 cm4")).unwrap_err();
        assert!(e.contains(why), "{o}: {e}");
    }
    let both = model("y+z+", "400 cm4") + "reference = [0, 0, 1]\n";
    assert!(Model::parse(&both).unwrap_err().contains("not both"));
}
