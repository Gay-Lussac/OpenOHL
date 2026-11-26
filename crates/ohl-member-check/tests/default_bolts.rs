// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The plugin fixes of 2026-11-26 (the owner's answers of 2026-11-25):
//! the default bolt sent with a member whose data gives no bolts
//! (`ohl_member_check::default_bolts`, `member-check-default-bolts`) and a
//! rolled angle's own leg sent to the bolt distances as a rolled edge
//! unless its connection says `rolled_edge = false`. Through the bundled
//! plugins, built for wasm32-wasip2 by the full check; without them these
//! tests skip (fail with `OHL_REQUIRE_PLUGIN_TESTS`).

use std::path::{Path, PathBuf};

use ohl_member_check::default_bolts::{Source, default_bolts};
use ohl_structures::bolts::BoltLibrary;
use ohl_structures_fe::Model;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Whether the bundled plugin `name` is built (else skip, or fail when the
/// plugin tests are required).
fn built(name: &str) -> bool {
    if repo()
        .join(format!(
            "plugins/codes/{name}/target/wasm32-wasip2/release/{name}.wasm"
        ))
        .is_file()
    {
        return true;
    }
    assert!(
        std::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "plugins/codes/{name} not built"
    );
    eprintln!("skipping: plugins/codes/{name} not built");
    false
}

const SECTIONS: &str = "[[material]]\nid = \"steel\"\nelasticity = \"210 GPa\"\n\
    yield = \"355 MPa\"\nultimate = \"490 MPa\"\n\
    [[section]]\nid = \"L40\"\nshape = \"equal_angle\"\nmaterial = \"steel\"\n\
    h = \"40 mm\"\nt = \"4 mm\"\nr = \"0 mm\"\n\
    [[section]]\nid = \"L45\"\nshape = \"equal_angle\"\nmaterial = \"steel\"\n\
    h = \"45 mm\"\nt = \"5 mm\"\nr = \"0 mm\"\n\
    [[section]]\nid = \"L50\"\nshape = \"equal_angle\"\nmaterial = \"steel\"\n\
    h = \"50 mm\"\nt = \"5 mm\"\nr = \"0 mm\"\n\
    [[section]]\nid = \"L20\"\nshape = \"equal_angle\"\nmaterial = \"steel\"\n\
    h = \"20 mm\"\nt = \"3 mm\"\nr = \"0 mm\"\n";

const BOLTS: &str = "format = \"ohl-bolts\"\nversion = 1\n\
    [[bolt]]\nid = \"M12\"\ndiameter = \"12 mm\"\nhole = \"13.5 mm\"\nshear_capacity = \"100 kN\"\n\
    [[bolt]]\nid = \"M16\"\ndiameter = \"16 mm\"\nhole = \"17.5 mm\"\nshear_capacity = \"100 kN\"\n\
    [[bolt]]\nid = \"M20\"\ndiameter = \"20 mm\"\nhole = \"22 mm\"\nshear_capacity = \"100 kN\"\n";

/// A bar of `section` under 20 kN of tension by `capacity`, plus a second
/// bar `n` with `n_design` (its `[member.design]`, "" none).
fn bars(section: &str, capacity: &str, n_design: &str) -> String {
    format!(
        "[analysis]\nnonlinear = false\ncapacity = \"{capacity}\"\n\
         [[node]]\nid = \"a\"\npoint = [\"0 m\", \"0 m\", \"0 m\"]\nsupport = \"pinned\"\n\
         [[node]]\nid = \"b\"\npoint = [\"1.5 m\", \"0 m\", \"0 m\"]\nsupport = [\"y\", \"z\"]\n\
         [[node]]\nid = \"c\"\npoint = [\"3 m\", \"0 m\", \"0 m\"]\nsupport = [\"y\", \"z\"]\n\
         [[member]]\nid = \"m\"\nnodes = [\"a\", \"b\"]\nsection = \"{section}\"\n\
         [[member]]\nid = \"n\"\nnodes = [\"b\", \"c\"]\nsection = \"{section}\"\n{n_design}\
         [[load]]\nnode = \"c\"\nforce = [\"20 kN\", \"0 kN\", \"0 kN\"]\n"
    )
}

fn model(text: &str, bolts: Option<&str>) -> Model {
    let lib = ohl_sections::SectionLibrary::parse(SECTIONS).unwrap();
    let mut m = Model::parse_with(text, &lib).unwrap();
    if let Some(b) = bolts {
        m.bolt_library = BoltLibrary::parse(b).unwrap();
    }
    ohl_member_check::find(m, None).unwrap()
}

/// The largest bolt of the library the leg's usual gauge takes, by the
/// plugin's own edge distance (ASCE 10-97 4.5.3, the toe rolled: 1.53 d
/// with the force per bolt its bearing cap): an L 40×4 takes the M12
/// (18.4 mm of the 20 mm to the toe), not the M16 (24.5 mm); the member
/// with bolts gets none. By SIA 263's Table 17 an L 45×5 takes the M16 (e₂
/// 20 mm of 22.5 mm), not the M20 (25 mm).
#[test]
fn the_largest_bolt_the_legs_usual_gauge_takes() {
    if !built("asce10") || !built("sia263") {
        return;
    }
    let with_bolts =
        "[member.design]\nbolts = { diameter = \"16 mm\", count = 4, hole = \"17.5 mm\" }\n";
    let m = model(&bars("L40", "asce10-97", with_bolts), Some(BOLTS));
    let d = default_bolts(&m);
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(m.members[d[0].member].id, "m");
    assert_eq!(d[0].source, Source::Leg, "{d:?}");
    assert_eq!(d[0].bolt.as_deref(), Some("M12"));
    assert!((d[0].diameter.unwrap() - 0.012).abs() < 1e-12);
    assert!((d[0].hole.unwrap() - 0.0135).abs() < 1e-12);
    assert!(
        d[0].why
            .contains("its 40 mm leg's usual gauge takes, M12 (12 mm)")
            && d[0]
                .why
                .contains("20 mm to the rolled toe, at least the code's edge distance 18.4 mm")
            && d[0].why.contains("the next larger, M16, does not fit"),
        "{}",
        d[0].why
    );
    let m = model(&bars("L45", "sia263-2013", with_bolts), Some(BOLTS));
    let d = default_bolts(&m);
    assert_eq!(d[0].bolt.as_deref(), Some("M16"), "{d:?}");
    assert!(d[0].why.contains("Table 17, e_2"), "{}", d[0].why);
}

/// Without a library: the structure's most used bolt (member n's 16 mm, 4 at each end);
/// with neither: none, the plugin's own default kept; a leg too small for
/// every library bolt: none, said — never a smaller hole without a bolt
/// that fits.
#[test]
fn the_most_used_bolt_and_none() {
    if !built("asce10") {
        return;
    }
    let with_bolts =
        "[member.design]\nbolts = { diameter = \"16 mm\", count = 4, hole = \"17.5 mm\" }\n";
    let d = default_bolts(&model(&bars("L40", "asce10-97", with_bolts), None));
    assert_eq!(d[0].source, Source::Structure, "{d:?}");
    assert!((d[0].diameter.unwrap() - 0.016).abs() < 1e-12);
    assert!((d[0].hole.unwrap() - 0.0175).abs() < 1e-12);
    assert!(
        d[0].why
            .starts_with("the structure's most used bolt, 16 mm (8 bolts at member ends")
            && d[0].why.contains("the structure has no bolt library"),
        "{}",
        d[0].why
    );
    let d = default_bolts(&model(&bars("L40", "asce10-97", ""), None));
    assert_eq!(d.len(), 2);
    assert!(
        d.iter()
            .all(|d| d.source == Source::None && d.diameter.is_none())
    );
    let d = default_bolts(&model(&bars("L20", "asce10-97", ""), Some(BOLTS)));
    assert_eq!(d[0].source, Source::None, "{d:?}");
    assert!(
        d[0].why
            .contains("its 20 mm leg's usual gauge takes no bolt of the structure's library")
            && d[0].why.contains("the smallest, M12 (12 mm)"),
        "{}",
        d[0].why
    );
}

/// The plugin takes the default bolt's hole and says where it came from:
/// the L 40×4's net section with two M12 holes (13.5 + 1.6 mm punched),
/// 0.9 × 355 MPa × 183.2 mm² = 58.53 kN; an older call (no default sent)
/// keeps the 20 mm bolt, 37.83 kN.
#[test]
fn the_plugin_takes_the_default_bolts_hole() {
    if !built("asce10") {
        return;
    }
    let m = model(&bars("L40", "asce10-97", ""), Some(BOLTS));
    let given = ohl_member_check::capacities(&m).unwrap();
    let g = given.iter().find(|g| g.id == "m").unwrap();
    let t = g
        .capacities
        .iter()
        .find(|c| c.0 == "tension")
        .map(|c| c.1)
        .unwrap();
    assert!((t - 0.9 * 355e6 * 183.2e-6).abs() < 1e-3, "{t} {g:?}");
    assert!(
        g.notes.iter().any(|n| n.contains(
            "OpenOHL's default bolt, the largest bolt of the \
                                             structure's library its 40 mm leg's usual gauge \
                                             takes, M12"
        )),
        "{:?}",
        g.notes
    );
}

const TOWER: &str = "id = \"T\"\nname = \"t\"\nkind = \"lattice\"\n\
    [[node]]\nid = \"N1\"\npoint = [\"0 m\", \"0 m\", \"0 m\"]\nsupport = \"fixed\"\n\
    [[node]]\nid = \"N2\"\npoint = [\"2 m\", \"0 m\", \"0 m\"]\nsupport = \"fixed\"\n\
    [[member]]\nid = \"D\"\nnodes = [\"N1\", \"N2\"]\nsection = \"L50\"\n\
    [[connection]]\nmember = \"D\"\njoint = \"N2\"\nleg = \"long\"\n\
    bolts = { count = 2, diameter = \"16 mm\", hole = \"17.5 mm\" }\n";

/// The rolled toe: a member's own leg of a rolled angle is a rolled edge
/// (ASCE 10-97 Eq. 4.5-6: 0.85 × 28.8 = 24.48 mm for an M16 in a 5 mm leg
/// without an analysis) unless its connection says `rolled_edge = false`
/// (Eq. 4.5-7: + 1.5875 mm); a plate stays sheared.
#[test]
fn a_rolled_angles_own_leg_is_a_rolled_edge() {
    if !built("asce10") {
        return;
    }
    let lib = ohl_sections::SectionLibrary::parse(SECTIONS).unwrap();
    let rules = |text: &str, part: Option<(f64, Option<f64>)>| {
        let m = Model::from_structure_type_with_joint_details(
            text,
            &lib,
            &BoltLibrary::default(),
            None,
        )
        .unwrap();
        let m = ohl_member_check::find(m, None).unwrap();
        ohl_member_check::joints::bolt_rules(&m, 0, 1, None, part)
            .unwrap()
            .unwrap()
    };
    let r = rules(TOWER, None);
    assert!(!r.sheared);
    assert!((r.edge - 0.85 * 0.0288).abs() < 1e-12, "{r:?}");
    assert!(r.words().contains("rolled edge"), "{}", r.words());
    let r = rules(&format!("{TOWER}rolled_edge = false\n"), None);
    assert!(r.sheared);
    assert!(
        (r.edge - (0.85 * 0.0288 + 0.0015875)).abs() < 1e-12,
        "{r:?}"
    );
    let r = rules(TOWER, Some((0.010, None)));
    assert!(r.sheared, "a plate stays sheared");
}
