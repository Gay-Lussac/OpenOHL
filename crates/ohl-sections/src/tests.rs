// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Properties of every shape against closed forms worked out here by hand
//! (rectangles, parallel axes, circles, regular polygons); the dimensions
//! are invented.

use super::*;

fn close(actual: f64, expected: f64, rel: f64) {
    let scale = expected.abs().max(1e-30);
    assert!(
        ((actual - expected) / scale).abs() <= rel,
        "{actual} differs from {expected} by more than {rel} (relative)"
    );
}

fn section(shape: Shape) -> Section {
    Section {
        id: "s".into(),
        name: String::new(),
        description: String::new(),
        shape,
        material: None,
        given: Given::default(),
    }
}

fn deg(a: f64) -> Angle {
    Angle::from_degrees(a)
}

/// Rectangles as (area, centroid y, centroid z, own ∫(y−yc)², own ∫(z−zc)²).
fn rect(y0: f64, z0: f64, y1: f64, z1: f64) -> [f64; 5] {
    let (w, h) = (y1 - y0, z1 - z0);
    [
        w * h,
        (y0 + y1) / 2.0,
        (z0 + z1) / 2.0,
        h * w.powi(3) / 12.0,
        w * h.powi(3) / 12.0,
    ]
}

/// Area, centroid, iy, iz, iyz of rectangles by parallel axes.
fn by_parallel_axes(rects: &[[f64; 5]]) -> (f64, [f64; 2], f64, f64, f64) {
    let a: f64 = rects.iter().map(|r| r[0]).sum();
    let yc = rects.iter().map(|r| r[0] * r[1]).sum::<f64>() / a;
    let zc = rects.iter().map(|r| r[0] * r[2]).sum::<f64>() / a;
    let iy = rects
        .iter()
        .map(|r| r[4] + r[0] * (r[2] - zc).powi(2))
        .sum();
    let iz = rects
        .iter()
        .map(|r| r[3] + r[0] * (r[1] - yc).powi(2))
        .sum();
    let iyz = rects.iter().map(|r| r[0] * (r[1] - yc) * (r[2] - zc)).sum();
    (a, [yc, zc], iy, iz, iyz)
}

#[test]
fn i_section_without_root_radius() {
    let (h, b, tw, tf) = (0.21, 0.11, 0.0061, 0.0093);
    let p = section(Shape::ISection {
        h,
        b,
        tw,
        tf,
        r: 0.0,
    })
    .properties();
    let hw = h - 2.0 * tf;
    close(p.area, 2.0 * b * tf + hw * tw, 1e-12);
    close(p.centroid[0], b / 2.0, 1e-12);
    close(p.centroid[1], h / 2.0, 1e-12);
    let iy = (b * h.powi(3) - (b - tw) * hw.powi(3)) / 12.0;
    let iz = 2.0 * tf * b.powi(3) / 12.0 + hw * tw.powi(3) / 12.0;
    close(p.iy, iy, 1e-10);
    close(p.iz, iz, 1e-10);
    assert_eq!(p.iyz, 0.0);
    close(p.iu, iy, 1e-10);
    close(p.iv, iz, 1e-10);
    assert_eq!(p.principal_angle, 0.0);
    close(p.wel_y.unwrap(), iy / (h / 2.0), 1e-10);
    close(p.wel_z.unwrap(), iz / (b / 2.0), 1e-10);
    close(
        p.wpl_y.unwrap(),
        b * tf * (h - tf) + tw * hw * hw / 4.0,
        1e-9,
    );
    close(
        p.wpl_z.unwrap(),
        2.0 * tf * b * b / 4.0 + hw * tw * tw / 4.0,
        1e-9,
    );
    close(
        p.it.unwrap(),
        (2.0 * b * tf.powi(3) + hw * tw.powi(3)) / 3.0,
        1e-12,
    );
    close(p.gyration_y, (iy / p.area).sqrt(), 1e-10);
    close(p.gyration_v, (iz / p.area).sqrt(), 1e-10);
    close(p.shear_area_z.unwrap(), hw * tw, 1e-12);
    close(p.shear_area_y.unwrap(), 2.0 * b * tf, 1e-12);
    let s = section(Shape::ISection {
        h,
        b,
        tw,
        tf,
        r: 0.0,
    });
    close(s.wind_width(deg(0.0)), h, 1e-12);
    close(s.wind_width(deg(90.0)), b, 1e-12);
    close(s.wind_area_per_length(deg(180.0)).square_meters(), h, 1e-12);
}

#[test]
fn i_section_root_radii() {
    let (h, b, tw, tf, r) = (0.21, 0.11, 0.0061, 0.0093, 0.012);
    let plain = section(Shape::ISection {
        h,
        b,
        tw,
        tf,
        r: 0.0,
    })
    .properties();
    let p = section(Shape::ISection { h, b, tw, tf, r }).properties();
    // One fillet: an r × r square less a quarter disc. From its corner,
    // along either leg: area, first moment, second moment.
    let af = (1.0 - PI / 4.0) * r * r;
    let sf = r.powi(3) * (5.0 / 6.0 - PI / 4.0);
    let i_corner = r.powi(4) * (1.0 - 5.0 * PI / 16.0);
    close(p.area, plain.area + 4.0 * af, 1e-12);
    // About y the fillet's corner is at h/2 − tf, the fillet towards the
    // centroid.
    let dy = h / 2.0 - tf;
    let fillet_y = dy * dy * af - 2.0 * dy * sf + i_corner;
    close(p.iy, plain.iy + 4.0 * fillet_y, 1e-10);
    // About z the corner is at tw/2, the fillet away from the centroid.
    let dz = tw / 2.0;
    let fillet_z = dz * dz * af + 2.0 * dz * sf + i_corner;
    close(p.iz, plain.iz + 4.0 * fillet_z, 1e-10);
    close(p.wel_y.unwrap(), p.iy / (h / 2.0), 1e-10);
    // Plastic modulus: the fillets' first moments about the centre added.
    close(
        p.wpl_y.unwrap(),
        plain.wpl_y.unwrap() + 4.0 * (dy * af - sf),
        1e-9,
    );
}

#[test]
fn equal_angle_without_root_radius() {
    let (h, t) = (0.061, 0.0071);
    let rects = [rect(0.0, 0.0, h, t), rect(0.0, t, t, h)];
    let (a, c, iy, iz, iyz) = by_parallel_axes(&rects);
    let p = section(Shape::EqualAngle { h, t, r: 0.0 }).properties();
    close(p.area, t * (2.0 * h - t), 1e-12);
    close(p.area, a, 1e-12);
    close(p.centroid[0], c[0], 1e-12);
    close(p.centroid[1], c[1], 1e-12);
    close(p.centroid[0], p.centroid[1], 1e-12);
    close(p.iy, iy, 1e-10);
    close(p.iz, iz, 1e-10);
    close(p.iyz, iyz, 1e-10);
    assert!(p.iyz < 0.0);
    // Equal legs: principal axes at 45°, iu, iv = iy ± |iyz|.
    close(p.principal_angle, PI / 4.0, 1e-12);
    close(p.iu, iy - iyz, 1e-10);
    close(p.iv, iy + iyz, 1e-10);
    close(p.gyration_v, ((iy + iyz) / a).sqrt(), 1e-10);
    close(p.wel_y.unwrap(), iy / (h - c[1]), 1e-10);
    // Plastic axis in the horizontal leg (b t ≥ A / 2): at A / (2 b).
    let zp = a / (2.0 * h);
    assert!(zp < t);
    let wpl = h * zp * zp / 2.0 + h * (t - zp).powi(2) / 2.0 + t * (h - t) * ((h + t) / 2.0 - zp);
    close(p.wpl_y.unwrap(), wpl, 1e-9);
    close(p.wpl_z.unwrap(), wpl, 1e-9);
    close(p.it.unwrap(), (2.0 * h - t) * t.powi(3) / 3.0, 1e-12);
    // Elastic moduli about u, v: the farthest corners along v and u.
    let (u, v) = ([0.5f64.sqrt(); 2], [-(0.5f64.sqrt()), 0.5f64.sqrt()]);
    let corners = [[0.0, 0.0], [h, 0.0], [h, t], [t, t], [t, h], [0.0, h]];
    let reach = |e: [f64; 2]| {
        corners
            .iter()
            .map(|q| ((q[0] - c[0]) * e[0] + (q[1] - c[1]) * e[1]).abs())
            .fold(0.0, f64::max)
    };
    close(p.wel_u.unwrap(), p.iu / reach(v), 1e-10);
    close(p.wel_v.unwrap(), p.iv / reach(u), 1e-10);
    // Wind: one leg's length across it, the diagonal extent at 45°.
    let s = section(Shape::EqualAngle { h, t, r: 0.0 });
    close(s.wind_width(deg(0.0)), h, 1e-12);
    close(s.wind_width(deg(90.0)), h, 1e-12);
    close(s.wind_width(deg(45.0)), 2.0f64.sqrt() * h, 1e-12);
    close(s.wind_width(deg(-45.0)), (h + t) / 2.0f64.sqrt(), 1e-12);
}

#[test]
fn angle_root_radius_and_unequal_legs() {
    let (h, b, t, r) = (0.081, 0.052, 0.0062, 0.0071);
    let plain = section(Shape::UnequalAngle { h, b, t, r: 0.0 }).properties();
    let p = section(Shape::UnequalAngle { h, b, t, r }).properties();
    close(p.area, plain.area + (1.0 - PI / 4.0) * r * r, 1e-12);
    let rects = [rect(0.0, 0.0, b, t), rect(0.0, t, t, h)];
    let (_, c, iy, iz, iyz) = by_parallel_axes(&rects);
    close(plain.centroid[0], c[0], 1e-12);
    close(plain.centroid[1], c[1], 1e-12);
    close(plain.iy, iy, 1e-10);
    close(plain.iz, iz, 1e-10);
    close(plain.iyz, iyz, 1e-10);
    // Principal values: the same trace and determinant.
    close(plain.iu + plain.iv, iy + iz, 1e-10);
    close(plain.iu * plain.iv, iy * iz - iyz * iyz, 1e-9);
    // u turned from y by α with tan 2α = −2 iyz / (iy − iz).
    let a2 = 2.0 * plain.principal_angle;
    close(a2.tan(), -2.0 * iyz / (iy - iz), 1e-9);
    close(plain.it.unwrap(), (h + b - t) * t.powi(3) / 3.0, 1e-12);
    let s = section(Shape::UnequalAngle { h, b, t, r });
    close(s.wind_width(deg(0.0)), h, 1e-12);
    close(s.wind_width(deg(90.0)), b, 1e-12);
}

#[test]
fn channel() {
    let (h, b, tw, tf) = (0.16, 0.065, 0.0075, 0.0105);
    let hw = h - 2.0 * tf;
    let rects = [
        rect(0.0, 0.0, tw, h),
        rect(tw, 0.0, b, tf),
        rect(tw, h - tf, b, h),
    ];
    let (a, c, iy, iz, _) = by_parallel_axes(&rects);
    let p = section(Shape::Channel {
        h,
        b,
        tw,
        tf,
        r: 0.0,
    })
    .properties();
    close(p.area, a, 1e-12);
    close(p.centroid[0], c[0], 1e-12);
    close(p.centroid[1], h / 2.0, 1e-12);
    close(p.iy, (b * h.powi(3) - (b - tw) * hw.powi(3)) / 12.0, 1e-10);
    close(p.iy, iy, 1e-10);
    close(p.iz, iz, 1e-10);
    assert_eq!(p.iyz, 0.0);
    close(p.wel_z.unwrap(), iz / c[0].max(b - c[0]), 1e-10);
    close(
        p.wpl_y.unwrap(),
        b * tf * (h - tf) + tw * hw * hw / 4.0,
        1e-9,
    );
    let with_r = section(Shape::Channel {
        h,
        b,
        tw,
        tf,
        r: 0.0095,
    })
    .properties();
    close(
        with_r.area,
        a + 2.0 * (1.0 - PI / 4.0) * 0.0095f64.powi(2),
        1e-12,
    );
    close(with_r.centroid[1], h / 2.0, 1e-12);
}

#[test]
fn rectangular_hollow() {
    let (h, b, t) = (0.12, 0.08, 0.005);
    let (hi, bi) = (h - 2.0 * t, b - 2.0 * t);
    let p = section(Shape::RectangularHollow { h, b, t }).properties();
    let a = b * h - bi * hi;
    close(p.area, a, 1e-12);
    let iy = (b * h.powi(3) - bi * hi.powi(3)) / 12.0;
    let iz = (h * b.powi(3) - hi * bi.powi(3)) / 12.0;
    close(p.iy, iy, 1e-10);
    close(p.iz, iz, 1e-10);
    close(p.wel_y.unwrap(), iy / (h / 2.0), 1e-10);
    close(p.wpl_y.unwrap(), (b * h * h - bi * hi * hi) / 4.0, 1e-9);
    close(p.wpl_z.unwrap(), (h * b * b - hi * bi * bi) / 4.0, 1e-9);
    let am = (h - t) * (b - t);
    close(
        p.it.unwrap(),
        4.0 * am * am * t / (2.0 * (h + b - 2.0 * t)),
        1e-12,
    );
    close(p.shear_area_y.unwrap() + p.shear_area_z.unwrap(), a, 1e-12);
    let s = section(Shape::RectangularHollow { h, b, t });
    close(s.wind_width(deg(0.0)), h, 1e-12);
    // The wind's normal along the diagonal: the diagonal's length.
    let diag = deg(-b.atan2(h).to_degrees());
    close(s.wind_width(diag), (h * h + b * b).sqrt(), 1e-12);
}

#[test]
fn round_shapes() {
    let (d, t) = (0.1683, 0.0071);
    let di = d - 2.0 * t;
    let p = section(Shape::CircularHollow { d, t }).properties();
    let i = PI * (d.powi(4) - di.powi(4)) / 64.0;
    close(p.area, PI * (d * d - di * di) / 4.0, 1e-12);
    close(p.iy, i, 1e-10);
    close(p.iz, i, 1e-10);
    close(p.iu, i, 1e-10);
    close(p.it.unwrap(), 2.0 * i, 1e-10);
    close(p.wel_y.unwrap(), i / (d / 2.0), 1e-10);
    close(p.wpl_y.unwrap(), (d.powi(3) - di.powi(3)) / 6.0, 1e-9);
    close(p.centroid[0], d / 2.0, 1e-12);
    close(p.shear_area_z.unwrap(), p.area / 2.0, 1e-12);
    let s = section(Shape::CircularHollow { d, t });
    for a in [0.0, 30.0, 77.0] {
        close(s.wind_width(deg(a)), d, 1e-12);
    }
    let bar = section(Shape::RoundBar { d: 0.03 }).properties();
    close(bar.area, PI * 0.03f64.powi(2) / 4.0, 1e-12);
    close(bar.iz, PI * 0.03f64.powi(4) / 64.0, 1e-10);
    close(bar.it.unwrap(), PI * 0.03f64.powi(4) / 32.0, 1e-10);
    close(bar.wpl_z.unwrap(), 0.03f64.powi(3) / 6.0, 1e-9);
    close(bar.gyration_y, 0.03 / 4.0, 1e-10);
}

#[test]
fn polygonal_hollow() {
    // A regular n-gon of inradius a: side s = 2 a tan(π/n), area n a² tan(π/n),
    // second moment about any centroidal axis A (12 a² + s²) / 48.
    let polygon = |n: f64, a: f64| {
        let s = 2.0 * a * (PI / n).tan();
        let area = n * a * a * (PI / n).tan();
        (area, area * (12.0 * a * a + s * s) / 48.0)
    };
    let (n, d, t) = (8, 0.42, 0.006);
    let (ao, io) = polygon(8.0, d / 2.0);
    let (ai, ii) = polygon(8.0, d / 2.0 - t);
    let p = section(Shape::PolygonalHollow { sides: n, d, t }).properties();
    close(p.area, ao - ai, 1e-12);
    close(p.iy, io - ii, 1e-10);
    close(p.iz, io - ii, 1e-10);
    assert_eq!(p.iyz, 0.0);
    // Bredt on the mid-wall polygon.
    let am_r = (d - t) / 2.0;
    let tan = (PI / 8.0).tan();
    let am = 8.0 * am_r * am_r * tan;
    close(
        p.it.unwrap(),
        4.0 * am * am * t / (16.0 * am_r * tan),
        1e-12,
    );
    // Octagon: a side at the top, so across flats vertically.
    close(p.wel_y.unwrap(), (io - ii) / (d / 2.0), 1e-10);
    let s = section(Shape::PolygonalHollow { sides: n, d, t });
    close(s.wind_width(deg(0.0)), d, 1e-12);
    close(s.wind_width(deg(22.5)), d / (PI / 8.0).cos(), 1e-12);
    // Four sides: the square tube.
    let sq = section(Shape::PolygonalHollow { sides: 4, d, t }).properties();
    let rhs = section(Shape::RectangularHollow { h: d, b: d, t }).properties();
    close(sq.area, rhs.area, 1e-12);
    close(sq.iy, rhs.iy, 1e-10);
    close(sq.wpl_y.unwrap(), rhs.wpl_y.unwrap(), 1e-9);
    close(sq.it.unwrap(), rhs.it.unwrap(), 1e-12);
}

const FILE: &str = r#"
format = "ohl-sections"
version = 1
name = "Test sections (invented)"
default_material = "steel-x"

[[material]]
id = "steel-x"
elasticity = "200 GPa"
density = "7800 kg/m3"
yield = "300 MPa"
partial_factors = { gamma_m0 = 1.0 }

[[section]]
id = "L-61x7"
shape = "equal_angle"
h = "61 mm"
t = "7 mm"

[[section]]
id = "box"
shape = "rectangular_hollow"
material = "other"
h = "4 in"
b = "2 in"
t = "0.25 in"
iy = "300 cm4"

[[section]]
id = "special"
shape = "general"
h = "100 mm"
b = "50 mm"
area = "1200 mm2"
iy = "250 cm4"
iz = "40 cm4"
"#;

#[test]
fn file_with_units_defaults_and_given_values() {
    let lib = SectionLibrary::parse(FILE).unwrap();
    assert_eq!(lib.name, "Test sections (invented)");
    let m = lib.material("steel-x").unwrap();
    close(m.elasticity, 200e9, 1e-12);
    close(m.shear_modulus, 200e9 / 2.6, 1e-12);
    assert_eq!(m.yield_strength, Some(300e6));
    assert_eq!(m.ultimate, None);
    assert_eq!(m.partial_factors["gamma_m0"], 1.0);
    let angle = lib.section("L-61x7").unwrap();
    assert_eq!(angle.material.as_deref(), Some("steel-x"));
    assert_eq!(lib.material_of(angle).unwrap().id, "steel-x");
    assert_eq!(
        angle.shape,
        Shape::EqualAngle {
            h: 0.061,
            t: 0.007,
            r: 0.0
        }
    );
    // Inches read as such; a given iy replaces the computed one and its
    // radius of gyration follows.
    let b = lib.section("box").unwrap();
    let inch = 0.0254;
    let computed = section(Shape::RectangularHollow {
        h: 4.0 * inch,
        b: 2.0 * inch,
        t: 0.25 * inch,
    })
    .properties();
    let p = b.properties();
    close(p.area, computed.area, 1e-12);
    close(p.iy, 300e-8, 1e-12);
    close(p.gyration_y, (300e-8 / p.area).sqrt(), 1e-12);
    close(p.iz, computed.iz, 1e-12);
    close(p.wel_z.unwrap(), computed.wel_z.unwrap(), 1e-12);
    assert_eq!(
        lib.missing_materials(),
        vec!["section \"box\": no material \"other\"".to_owned()]
    );
    let g = lib.section("special").unwrap().properties();
    close(g.area, 1200e-6, 1e-12);
    close(g.iu, 250e-8, 1e-12);
    close(g.iv, 40e-8, 1e-12);
    assert_eq!(g.it, None);
    assert_eq!(g.wel_y, None);
    close(
        lib.section("special").unwrap().wind_width(deg(0.0)),
        0.1,
        1e-12,
    );
}

#[test]
fn mistakes_are_refused_by_name() {
    let err = |text: &str| SectionLibrary::parse(text).unwrap_err();
    let e = err(&FILE.replace("t = \"7 mm\"", "t = \"7 mm\"\nthick = 1"));
    assert!(e.contains("thick"), "{e}");
    let e = err(&FILE.replace("name = \"Test", "nme = \"x\"\nname = \"Test"));
    assert!(e.contains("nme"), "{e}");
    let e = err(&FILE.replace("density = ", "densty = "));
    assert!(e.contains("densty"), "{e}");
    let e = err(&FILE.replace("t = \"7 mm\"", "t = \"7 mm\"\nb = \"50 mm\""));
    assert!(
        e.contains("section \"L-61x7\": b is not a dimension of equal_angle"),
        "{e}"
    );
    let e = err(&FILE.replace("equal_angle", "angel"));
    assert!(e.contains("shape \"angel\" unknown"), "{e}");
    let e = err(&FILE.replace("t = \"7 mm\"", "t = \"7\""));
    assert!(e.contains("unit"), "{e}");
    let e = err(&FILE.replace("t = \"7 mm\"", "t = \"7 kg\""));
    assert!(e.contains("kg"), "{e}");
    let e = err(&FILE.replace("t = \"7 mm\"\n", ""));
    assert!(e.contains("equal_angle needs t"), "{e}");
    let e = err(&FILE.replace("t = \"7 mm\"", "t = \"70 mm\""));
    assert!(e.contains("t + r"), "{e}");
    let e = err(&FILE.replace("area = \"1200 mm2\"\n", ""));
    assert!(e.contains("a general section needs area"), "{e}");
    let e = err(&FILE.replace("id = \"box\"", "id = \"L-61x7\""));
    assert!(e.contains("section \"L-61x7\" is defined twice"), "{e}");
    let e = err(&FILE.replace("id = \"box\"", "id = \"a box\""));
    assert!(e.contains("letters, digits"), "{e}");
    let e = err(&FILE.replace("version = 1", "version = 9"));
    assert!(e.contains("newer"), "{e}");
    let e = err(&FILE.replace("\"ohl-sections\"", "\"ohl-wire\""));
    assert!(e.contains("is not \"ohl-sections\""), "{e}");
}

#[test]
fn libraries_merge_the_earlier_first() {
    let mut project = SectionLibrary::parse(FILE).unwrap();
    let user = SectionLibrary::parse(
        r#"
        name = "User"
        [[material]]
        id = "other"
        elasticity = "70 GPa"
        [[section]]
        id = "L-61x7"
        shape = "round_bar"
        d = "20 mm"
        [[section]]
        id = "bar"
        shape = "round_bar"
        d = "20 mm"
        "#,
    )
    .unwrap();
    let left_out = project.merge(user);
    assert_eq!(left_out, vec!["L-61x7".to_owned()]);
    assert_eq!(project.name, "Test sections (invented)");
    assert!(matches!(
        project.section("L-61x7").unwrap().shape,
        Shape::EqualAngle { .. }
    ));
    assert!(project.section("bar").is_some());
    assert_eq!(project.section("bar").unwrap().material, None);
    assert!(project.missing_materials().is_empty());
}

#[test]
fn the_shipped_example_reads() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../libraries/sections")
        .join(format!("example-sections.{SECTION_FILE_EXTENSION}"));
    let lib = SectionLibrary::load(&path).unwrap();
    assert!(lib.missing_materials().is_empty());
    assert!(lib.sections.len() >= 8);
    for s in &lib.sections {
        let p = s.properties();
        assert!(
            p.area > 0.0 && p.iy > 0.0 && p.iz > 0.0 && p.iv > 0.0,
            "{}",
            s.id
        );
        assert!(s.wind_width(deg(0.0)) > 0.0, "{}", s.id);
    }
    let folder = SectionLibrary::load(path.parent().unwrap()).unwrap();
    assert_eq!(folder.sections.len(), lib.sections.len());
}

/// Wind factors of the `[wind]` table (numbers invented): read, checked,
/// interpolated by hand.
const WIND: &str = r#"
[wind]
source = "invented test values"
drag = { equal_angle = 2.0, circular_hollow = 1.2 }

[wind.face_drag]
solidity = [0.1, 0.3, 0.5]
factor = [3.0, 2.6, 2.2]

[wind.shielding]
solidity = [0.1, 0.5]
spacing = [1.0, 3.0]
factor = [[0.8, 0.4], [1.0, 0.6]]
"#;

#[test]
fn wind_factors_read_and_interpolate() {
    let lib = SectionLibrary::parse(WIND).unwrap();
    let w = lib.wind.as_ref().unwrap();
    assert_eq!(w.source_name(), "invented test values");
    assert_eq!(w.drag_of("equal_angle"), Some(2.0));
    assert_eq!(w.drag_of("channel"), None);
    let face = w.face_drag.as_ref().unwrap();
    // Halfway between 0.1 (3.0) and 0.3 (2.6): 2.8.
    close(face.at(0.2).unwrap(), 2.8, 1e-12);
    close(face.at(0.5).unwrap(), 2.2, 1e-12);
    assert_eq!(face.at(0.05), None, "below the table: none");
    assert_eq!(face.at(0.6), None, "above the table: none");
    let s = w.shielding.as_ref().unwrap();
    // φ = 0.3: rows 0.6 (spacing 1) and 0.8 (spacing 3); spacing 2: 0.7.
    close(s.at(0.3, 2.0).unwrap(), 0.7, 1e-12);
    close(s.at(0.1, 1.0).unwrap(), 0.8, 1e-12);
    assert_eq!(s.at(0.3, 4.0), None, "spacing beyond the table");
    // Without spacing: one row, the spacing ignored.
    let flat =
        SectionLibrary::parse("[wind.shielding]\nsolidity = [0, 1]\nfactor = [1, 0.5]\n").unwrap();
    let s = flat.wind.unwrap().shielding.unwrap();
    close(s.at(0.5, 99.0).unwrap(), 0.75, 1e-12);
    // A library without [wind] has none.
    assert_eq!(SectionLibrary::parse("").unwrap().wind, None);
}

#[test]
fn wind_mistakes_are_refused() {
    for (text, says) in [
        ("[wind]\ndrag = { angle = 2 }", "not a shape"),
        ("[wind]\ndrag = { round_bar = 0 }", "above 0"),
        (
            "[wind.face_drag]\nsolidity = [0.3, 0.1]\nfactor = [1, 2]",
            "rise",
        ),
        (
            "[wind.face_drag]\nsolidity = [0.1]\nfactor = [1]",
            "two values",
        ),
        (
            "[wind.face_drag]\nsolidity = [0.1, 1.2]\nfactor = [1, 2]",
            "between 0 and 1",
        ),
        (
            "[wind.face_drag]\nsolidity = [0.1, 0.2]\nfactor = [1]",
            "one factor per solidity",
        ),
        (
            "[wind.shielding]\nsolidity = [0.1, 0.2]\nfactor = [1, 1.5]",
            "between 0 and 1",
        ),
        (
            "[wind.shielding]\nsolidity = [0.1, 0.2]\nfactor = [[1, 1]]",
            "need spacing",
        ),
        (
            "[wind.shielding]\nsolidity = [0.1, 0.2]\nspacing = [1, 2]\nfactor = [[1, 1]]",
            "one row of factors per spacing",
        ),
        ("[wind]\nshade = 1", "unknown field"),
    ] {
        let e = SectionLibrary::parse(text).unwrap_err();
        assert!(e.contains(says), "{text}: {e}");
    }
}

#[test]
fn one_wind_table_per_library() {
    let dir = std::env::temp_dir().join(format!("ohl-sections-wind-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.ohlsec"), WIND).unwrap();
    std::fs::write(dir.join("b.ohlsec"), "[wind]\ndrag = { round_bar = 1.1 }\n").unwrap();
    let e = SectionLibrary::load(&dir).unwrap_err();
    assert!(e.contains("[wind]") && e.contains("b.ohlsec"), "{e}");
    std::fs::remove_file(dir.join("b.ohlsec")).unwrap();
    let lib = SectionLibrary::load(&dir).unwrap();
    assert_eq!(lib.wind.unwrap().drag_of("equal_angle"), Some(2.0));
    std::fs::remove_dir_all(&dir).unwrap();
    // Merged: the earlier one stays, the later one is reported.
    let mut a = SectionLibrary::parse(WIND).unwrap();
    let b = SectionLibrary::parse("[wind]\ndrag = { round_bar = 1.1 }\n").unwrap();
    assert_eq!(a.merge(b), vec![WIND_TABLE.to_owned()]);
    assert_eq!(a.wind.unwrap().drag_of("round_bar"), None);
}
