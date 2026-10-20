// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Drawn structure bodies (levels 2 and 3 in the line's 3D view and
//! drawings): the geometry made once per type, then only moved to each
//! placed structure. Placing a lattice tower of 178 members with solids
//! must stay far below a millisecond, so a line of hundreds of towers
//! builds its structures layer in a few milliseconds.

use std::hint::black_box;
use std::path::Path;
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use ohl_sections::SectionLibrary;
use ohl_structures::StructureLibrary;
use ohl_structures_fe::drawn::Bodies;

fn library() -> (StructureLibrary, SectionLibrary) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let types =
        StructureLibrary::load_dir(&root.join("examples/synthetic-line/Structures")).unwrap();
    // The example's legs, braces and arms as invented library angles.
    let sections = SectionLibrary::parse(
        "format = \"ohl-sections\"\nversion = 1\nname = \"b\"\ndefault_material = \"s\"\n\
         [[material]]\nid = \"s\"\nelasticity = \"210 GPa\"\ndensity = \"7850 kg/m3\"\n\
         [[section]]\nid = \"leg\"\nshape = \"equal_angle\"\nh = \"100 mm\"\nt = \"10 mm\"\nr = \"0 mm\"\n\
         [[section]]\nid = \"brace\"\nshape = \"equal_angle\"\nh = \"60 mm\"\nt = \"6 mm\"\nr = \"0 mm\"\n\
         [[section]]\nid = \"arm\"\nshape = \"equal_angle\"\nh = \"80 mm\"\nt = \"8 mm\"\nr = \"0 mm\"\n",
    )
    .unwrap();
    (types, sections)
}

fn load(c: &mut Criterion) {
    let (types, sections) = library();
    c.bench_function("drawn/bodies_load_example", |b| {
        b.iter(|| Bodies::load(black_box(&types), &sections))
    });
}

fn place(c: &mut Criterion) {
    let (types, sections) = library();
    let bodies = Bodies::load(&types, &sections);
    let body = bodies.get("type-02", None).unwrap();
    assert!(body.triangles() > 0);
    c.bench_function("drawn/place_lattice_tower_100x", |b| {
        b.iter(|| {
            let mut n = 0;
            for k in 0..100 {
                let at = [400.0 * f64::from(k), 0.0, 500.0];
                n += body.placed_lines(black_box(at), 30.0).len();
                n += body.placed_triangles(black_box(at), 30.0).len();
            }
            n
        })
    });
}

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(2))
}

criterion_group! {
    name = benches;
    config = config();
    targets = load, place
}
criterion_main!(benches);
