// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Built-up sections (two angles back to back, in a star, four in a cross)
//! against their angles moved by the parallel-axis theorem, worked out here
//! by hand; the dimensions are invented.

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

/// One equal angle (legs `h`, thickness `t`, no root radius), its heel at
/// the origin, worked out by hand from its two plates: (area A1, centroid
/// e from the heel along y and z, own second moment I1 about either
/// centroidal axis parallel to a leg, own product I1yz, negative for legs
/// along +y and +z).
///
/// Plate 1: [0, h] × [0, t], area h t, centroid (h/2, t/2); plate 2:
/// [0, t] × [t, h], area t (h − t), centroid (t/2, (h + t)/2). Then
/// e = (h t · h/2 + t (h − t) · t/2) / A1, and by parallel axes
/// I1 = h t³/12 + h t (t/2 − e)² + t (h − t)³/12 + t (h − t) ((h + t)/2 − e)²,
/// I1yz = h t (h/2 − e)(t/2 − e) + t (h − t)(t/2 − e)((h + t)/2 − e).
fn equal_angle_by_hand(h: f64, t: f64) -> (f64, f64, f64, f64) {
    let a1 = t * (2.0 * h - t);
    let e = (h * t * h / 2.0 + t * (h - t) * t / 2.0) / a1;
    let i1 = h * t.powi(3) / 12.0
        + h * t * (t / 2.0 - e).powi(2)
        + t * (h - t).powi(3) / 12.0
        + t * (h - t) * ((h + t) / 2.0 - e).powi(2);
    let i1yz =
        h * t * (h / 2.0 - e) * (t / 2.0 - e) + t * (h - t) * (t / 2.0 - e) * ((h + t) / 2.0 - e);
    (a1, e, i1, i1yz)
}

/// Two equal angles back to back, invented legs 63 × 5.8 mm, 8.5 mm apart.
/// About the symmetry axis (y = 0 at the middle of the gap), the right
/// angle's centroid lies at (g/2 + e, e), the left one mirrored:
/// A = 2 A1; centroid e above the bottom; Iy = 2 I1 (both centroids at the
/// same height); Iz = 2 (I1 + A1 (g/2 + e)²); iyz = 0 (mirrored).
/// Wel,y = Iy / (h − e) (the leg tips farthest), Wel,z = Iz / (h + g/2).
/// Plastic: about z the symmetry axis halves the area, Wpl,z = 2 A1 (g/2 +
/// e); about y both angles' plastic axis is a single angle's, in the
/// outstanding legs at zp = A1 / (2 h): Wpl,y = 2 [h zp²/2 + h (t − zp)²/2
/// + t (h − t)((h + t)/2 − zp)]. It = 2 (2h − t) t³/3.
#[test]
fn double_angle_by_parallel_axes() {
    let (h, t, gap) = (0.063, 0.0058, 0.0085);
    let (a1, e, i1, i1yz) = equal_angle_by_hand(h, t);
    let shape = Shape::DoubleAngle {
        h,
        b: h,
        t,
        r: 0.0,
        gap,
    };
    let s = section(shape.clone());
    let p = s.properties();
    let d = gap / 2.0 + e;
    close(p.area, 2.0 * a1, 1e-12);
    close(p.centroid[0], h + gap / 2.0, 1e-12);
    close(p.centroid[1], e, 1e-12);
    close(p.width, 2.0 * h + gap, 1e-12);
    close(p.depth, h, 1e-12);
    close(p.iy, 2.0 * i1, 1e-10);
    close(p.iz, 2.0 * (i1 + a1 * d * d), 1e-10);
    assert_eq!(p.iyz, 0.0);
    close(p.iu, p.iz, 1e-12);
    close(p.iv, p.iy, 1e-12);
    close(p.wel_y.unwrap(), 2.0 * i1 / (h - e), 1e-10);
    close(p.wel_z.unwrap(), p.iz / (h + gap / 2.0), 1e-10);
    close(p.wpl_z.unwrap(), 2.0 * a1 * d, 1e-9);
    let zp = a1 / (2.0 * h);
    assert!(zp < t);
    let wpl = h * zp * zp / 2.0 + h * (t - zp).powi(2) / 2.0 + t * (h - t) * ((h + t) / 2.0 - zp);
    close(p.wpl_y.unwrap(), 2.0 * wpl, 1e-9);
    close(p.it.unwrap(), 2.0 * (2.0 * h - t) * t.powi(3) / 3.0, 1e-12);
    close(p.shear_area_y.unwrap(), 2.0 * h * t, 1e-12);
    close(p.shear_area_z.unwrap(), 2.0 * h * t, 1e-12);
    // Wind along y sees the legs' height; along z both legs and the gap.
    close(s.wind_width(deg(0.0)), h, 1e-12);
    close(s.wind_width(deg(90.0)), 2.0 * h + gap, 1e-12);
    // Each angle's own minor axis: iv = I1 + I1yz (I1yz < 0).
    let part = s.part_properties().unwrap();
    close(part.area, a1, 1e-12);
    close(part.gyration_v, ((i1 + i1yz) / a1).sqrt(), 1e-10);
    assert!(part.gyration_v < p.gyration_v);
    assert_eq!(shape.part().unwrap().1, 2);
}

/// Two equal angles in a star (the same invented angle): the centroid at
/// the centre, the angles' centroids at ±(d, d), d = g/2 + e. Iy = Iz =
/// 2 (I1 + A1 d²); iyz = 2 (I1yz + A1 d²) (a point reflection keeps an
/// angle's own product); principal axes at 45°: iv = Iy − iyz about the
/// diagonal through both angles, iu = Iy + iyz. Wel,y = Iy / (h + g/2).
/// The centre halves the area: Wpl,y = Wpl,z = 2 A1 d.
#[test]
fn star_angle_by_parallel_axes() {
    let (h, t, gap) = (0.063, 0.0058, 0.0085);
    let (a1, e, i1, i1yz) = equal_angle_by_hand(h, t);
    let s = section(Shape::StarAngle {
        h,
        b: h,
        t,
        r: 0.0,
        gap,
    });
    let p = s.properties();
    let d = gap / 2.0 + e;
    let i = 2.0 * (i1 + a1 * d * d);
    let iyz = 2.0 * (i1yz + a1 * d * d);
    assert!(iyz > 0.0);
    close(p.area, 2.0 * a1, 1e-12);
    close(p.centroid[0], h + gap / 2.0, 1e-12);
    close(p.centroid[1], h + gap / 2.0, 1e-12);
    close(p.iy, i, 1e-10);
    close(p.iz, i, 1e-10);
    close(p.iyz, iyz, 1e-10);
    close(p.iu, i + iyz, 1e-10);
    close(p.iv, i - iyz, 1e-10);
    close(p.principal_angle, -PI / 4.0, 1e-12);
    close(p.wel_y.unwrap(), i / (h + gap / 2.0), 1e-10);
    close(p.wpl_y.unwrap(), 2.0 * a1 * d, 1e-9);
    close(p.wpl_z.unwrap(), 2.0 * a1 * d, 1e-9);
    close(p.it.unwrap(), 2.0 * (2.0 * h - t) * t.powi(3) / 3.0, 1e-12);
    close(s.wind_width(deg(0.0)), 2.0 * h + gap, 1e-12);
    close(s.wind_width(deg(90.0)), 2.0 * h + gap, 1e-12);
}

/// Four equal angles in a cross (the same invented angle): their centroids
/// at (±d, ±d), d = g/2 + e. Iy = Iz = 4 (I1 + A1 d²); the products cancel
/// pairwise (iyz = 0), every axis through the centre principal. Wel = Iy /
/// (h + g/2), Wpl = 4 A1 d, It = 4 (2h − t) t³/3.
#[test]
fn cross_angle_by_parallel_axes() {
    let (h, t, gap) = (0.063, 0.0058, 0.0085);
    let (a1, e, i1, _) = equal_angle_by_hand(h, t);
    let s = section(Shape::CrossAngle { h, t, r: 0.0, gap });
    let p = s.properties();
    let d = gap / 2.0 + e;
    let i = 4.0 * (i1 + a1 * d * d);
    close(p.area, 4.0 * a1, 1e-12);
    close(p.centroid[0], h + gap / 2.0, 1e-12);
    close(p.centroid[1], h + gap / 2.0, 1e-12);
    close(p.iy, i, 1e-10);
    close(p.iz, i, 1e-10);
    assert_eq!(p.iyz, 0.0);
    close(p.iv, i, 1e-10);
    close(p.wel_y.unwrap(), i / (h + gap / 2.0), 1e-10);
    close(p.wpl_y.unwrap(), 4.0 * a1 * d, 1e-9);
    close(p.it.unwrap(), 4.0 * (2.0 * h - t) * t.powi(3) / 3.0, 1e-12);
    close(p.shear_area_y.unwrap(), 4.0 * h * t, 1e-12);
    close(s.wind_width(deg(0.0)), 2.0 * h + gap, 1e-12);
    // Root radii: each angle's fillet, (1 − π/4) r², four times.
    let r = 0.007;
    let rounded = section(Shape::CrossAngle { h, t, r, gap }).properties();
    close(rounded.area, 4.0 * (a1 + (1.0 - PI / 4.0) * r * r), 1e-12);
}

/// Unequal legs and root radii: the built-up sections against their single
/// angle (computed by the library, tested in `tests.rs`) moved by parallel
/// axes. Invented angle 82 × 54 × 6.4 mm, root radius 7 mm, gap 11 mm.
#[test]
fn unequal_built_up_from_their_angle() {
    let (h, b, t, r, gap) = (0.082, 0.054, 0.0064, 0.007, 0.011);
    let one = section(Shape::UnequalAngle { h, b, t, r }).properties();
    let (a1, [ey, ez]) = (one.area, one.centroid);
    // Back to back, legs h together: y offsets ±(g/2 + ey), z the same.
    let p = section(Shape::DoubleAngle { h, b, t, r, gap }).properties();
    close(p.area, 2.0 * a1, 1e-12);
    close(p.iy, 2.0 * one.iy, 1e-10);
    close(p.iz, 2.0 * (one.iz + a1 * (gap / 2.0 + ey).powi(2)), 1e-10);
    assert_eq!(p.iyz, 0.0);
    // In a star: both centroids off the centre along y and z.
    let (dy, dz) = (gap / 2.0 + ey, gap / 2.0 + ez);
    let p = section(Shape::StarAngle { h, b, t, r, gap }).properties();
    close(p.iy, 2.0 * (one.iy + a1 * dz * dz), 1e-10);
    close(p.iz, 2.0 * (one.iz + a1 * dy * dy), 1e-10);
    close(p.iyz, 2.0 * (one.iyz + a1 * dy * dz), 1e-10);
    close(p.iu + p.iv, p.iy + p.iz, 1e-10);
    close(p.iu * p.iv, p.iy * p.iz - p.iyz * p.iyz, 1e-9);
}

#[test]
fn built_up_shapes_read_from_files() {
    let read = |body: &str| {
        SectionLibrary::parse(&format!("[[section]]\nid = \"B\"\n{body}"))
            .map(|l| l.sections[0].clone())
    };
    // Unequal legs: `back` picks the legs back to back (h, along z).
    let s = read(
        "shape = \"double_angle\"\nh = \"80 mm\"\nb = \"60 mm\"\nt = \"6 mm\"\n\
         gap = \"10 mm\"\nback = \"short\"",
    )
    .unwrap();
    assert_eq!(
        s.shape,
        Shape::DoubleAngle {
            h: 0.06,
            b: 0.08,
            t: 0.006,
            r: 0.0,
            gap: 0.01
        }
    );
    let s = read(
        "shape = \"double_angle\"\nh = \"60 mm\"\nb = \"80 mm\"\nt = \"6 mm\"\n\
         gap = \"10 mm\"\nback = \"long\"",
    )
    .unwrap();
    assert!(matches!(s.shape, Shape::DoubleAngle { h, b, .. } if h == 0.08 && b == 0.06));
    // Equal legs: b and back may be left out; a gap of 0: the angles
    // touching.
    let s = read("shape = \"star_angle\"\nh = \"70 mm\"\nt = \"7 mm\"\ngap = \"0 mm\"").unwrap();
    assert!(matches!(s.shape, Shape::StarAngle { h, b, gap, .. } if h == b && gap == 0.0));
    let s =
        read("shape = \"cross_angle\"\nh = \"70 mm\"\nt = \"7 mm\"\nr = \"9 mm\"\ngap = \"12 mm\"")
            .unwrap();
    assert_eq!(s.shape.as_str(), "cross_angle");
    assert!(s.shape.is_built_up());
    // Refused, said by name.
    for (body, says) in [
        (
            "shape = \"cross_angle\"\nh = \"70 mm\"\nt = \"7 mm\"",
            "cross_angle needs gap",
        ),
        (
            "shape = \"star_angle\"\nh = \"70 mm\"\nt = \"7 mm\"\ngap = \"-1 mm\"",
            "gap must be 0 or above",
        ),
        (
            "shape = \"double_angle\"\nh = \"70 mm\"\nb = \"50 mm\"\nt = \"7 mm\"\ngap = \"8 mm\"",
            "needs back",
        ),
        (
            "shape = \"double_angle\"\nh = \"70 mm\"\nt = \"7 mm\"\ngap = \"8 mm\"\nback = \"side\"",
            "\"long\" or \"short\"",
        ),
        (
            "shape = \"star_angle\"\nh = \"70 mm\"\nt = \"7 mm\"\ngap = \"8 mm\"\nback = \"long\"",
            "back is not a dimension of star_angle",
        ),
        (
            "shape = \"cross_angle\"\nh = \"70 mm\"\nb = \"60 mm\"\nt = \"7 mm\"\ngap = \"8 mm\"",
            "b is not a dimension of cross_angle",
        ),
        (
            "shape = \"equal_angle\"\nh = \"70 mm\"\nt = \"7 mm\"\ngap = \"8 mm\"",
            "gap is not a dimension",
        ),
        (
            "shape = \"cross_angle\"\nh = \"70 mm\"\nt = \"65 mm\"\nr = \"9 mm\"\ngap = \"8 mm\"",
            "t + r",
        ),
    ] {
        let e = read(body).unwrap_err();
        assert!(e.contains(says), "{body}: {e}");
    }
}

/// The drawings' outline: counter-clockwise, its box the section's.
#[test]
fn built_up_outlines_for_drawings() {
    let (h, b, t, r, gap) = (0.082, 0.054, 0.0064, 0.007, 0.011);
    for shape in [
        Shape::DoubleAngle { h, b, t, r, gap },
        Shape::StarAngle { h, b, t, r, gap },
        Shape::CrossAngle { h, t, r, gap },
    ] {
        let o = shape.built_up_outline().unwrap();
        let twice: f64 = (0..o.len())
            .map(|k| {
                let (p, q) = (o[k], o[(k + 1) % o.len()]);
                p[0] * q[1] - q[0] * p[1]
            })
            .sum();
        assert!(twice > 0.0, "{}", shape.as_str());
        let p = section(shape.clone()).properties();
        let max = |k: usize| o.iter().map(|v| v[k]).fold(f64::NEG_INFINITY, f64::max);
        let min = |k: usize| o.iter().map(|v| v[k]).fold(f64::INFINITY, f64::min);
        close(max(0) - min(0), p.width, 1e-12);
        close(max(1) - min(1), p.depth, 1e-12);
        assert_eq!([min(0), min(1)], [0.0, 0.0], "{}", shape.as_str());
        // Filled: at least the angles' area, the fillets left out.
        assert!(twice / 2.0 >= p.area * 0.95, "{}", shape.as_str());
    }
    assert!(Shape::EqualAngle { h, t, r }.built_up_outline().is_none());
}
