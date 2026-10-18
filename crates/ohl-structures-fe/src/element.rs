// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Element matrices in local axes (x along the member from node i to j).
//! Degrees of freedom per node: u, v, w, θx, θy, θz.
//!
//! - Elastic stiffness of the 3D Euler–Bernoulli beam: Przemieniecki,
//!   "Theory of Matrix Structural Analysis" (1968), §5.
//! - Geometric stiffness (axial force N, tension positive; the cubic
//!   shape functions): McGuire, Gallagher & Ziemian, "Matrix Structural
//!   Analysis" (2nd ed., 2000), §9.3; torsion–bending coupling terms are
//!   left out.

use nalgebra::{Matrix3, SMatrix, Vector3};

use crate::model::{Member, Model, Section, cross, norm, sub};

/// 12 × 12 element matrix.
pub(crate) type Mat12 = SMatrix<f64, 12, 12>;

/// Rows: the local x, y, z axes in structure axes.
pub(crate) fn axes(model: &Model, m: &Member) -> Matrix3<f64> {
    let d = sub(model.nodes[m.j].point, model.nodes[m.i].point);
    let l = norm(d);
    let x = [d[0] / l, d[1] / l, d[2] / l];
    let r = m.reference.unwrap_or(if x[2].abs() > 0.999 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    });
    // z ⟂ x and r; y = z × x lies in the plane of x and r.
    let z = cross(x, r);
    let nz = norm(z);
    let z = [z[0] / nz, z[1] / nz, z[2] / nz];
    let y = cross(z, x);
    let (s, c) = m.roll.sin_cos();
    let y2 = [
        c * y[0] + s * z[0],
        c * y[1] + s * z[1],
        c * y[2] + s * z[2],
    ];
    let z2 = [
        -s * y[0] + c * z[0],
        -s * y[1] + c * z[1],
        -s * y[2] + c * z[2],
    ];
    Matrix3::new(x[0], x[1], x[2], y2[0], y2[1], y2[2], z2[0], z2[1], z2[2])
}

/// The 12 × 12 transformation (global → local) from the axes.
pub(crate) fn transformation(r: &Matrix3<f64>) -> Mat12 {
    let mut t = Mat12::zeros();
    for b in 0..4 {
        t.fixed_view_mut::<3, 3>(3 * b, 3 * b).copy_from(r);
    }
    t
}

/// Elastic stiffness of a frame member, local axes.
pub(crate) fn frame_elastic(l: f64, s: &Section) -> Mat12 {
    let mut k = Mat12::zeros();
    let mut set = |i: usize, j: usize, v: f64| {
        k[(i, j)] = v;
        k[(j, i)] = v;
    };
    let a = s.e * s.area / l;
    set(0, 0, a);
    set(0, 6, -a);
    set(6, 6, a);
    let t = s.g * s.j / l;
    set(3, 3, t);
    set(3, 9, -t);
    set(9, 9, t);
    // Bending in the x–y plane (v, θz): about z.
    let ez = s.e * s.iz;
    let (c1, c2, c3, c4) = (
        12.0 * ez / l.powi(3),
        6.0 * ez / l.powi(2),
        4.0 * ez / l,
        2.0 * ez / l,
    );
    set(1, 1, c1);
    set(1, 5, c2);
    set(1, 7, -c1);
    set(1, 11, c2);
    set(5, 5, c3);
    set(5, 7, -c2);
    set(5, 11, c4);
    set(7, 7, c1);
    set(7, 11, -c2);
    set(11, 11, c3);
    // Bending in the x–z plane (w, θy): about y, with w' = −θy.
    let ey = s.e * s.iy;
    let (d1, d2, d3, d4) = (
        12.0 * ey / l.powi(3),
        6.0 * ey / l.powi(2),
        4.0 * ey / l,
        2.0 * ey / l,
    );
    set(2, 2, d1);
    set(2, 4, -d2);
    set(2, 8, -d1);
    set(2, 10, -d2);
    set(4, 4, d3);
    set(4, 8, d2);
    set(4, 10, d4);
    set(8, 8, d1);
    set(8, 10, d2);
    set(10, 10, d3);
    k
}

/// Geometric stiffness of a frame member under axial force `n` (tension
/// positive), local axes.
pub(crate) fn frame_geometric(l: f64, n: f64) -> Mat12 {
    let mut k = Mat12::zeros();
    let mut set = |i: usize, j: usize, v: f64| {
        k[(i, j)] = n / l * v;
        k[(j, i)] = n / l * v;
    };
    let (a, b, c, d) = (1.2, l / 10.0, 2.0 * l * l / 15.0, -l * l / 30.0);
    // (v, θz) plane.
    set(1, 1, a);
    set(1, 5, b);
    set(1, 7, -a);
    set(1, 11, b);
    set(5, 5, c);
    set(5, 7, -b);
    set(5, 11, d);
    set(7, 7, a);
    set(7, 11, -b);
    set(11, 11, c);
    // (w, θy) plane: the coupling terms change sign.
    set(2, 2, a);
    set(2, 4, -b);
    set(2, 8, -a);
    set(2, 10, -b);
    set(4, 4, c);
    set(4, 8, b);
    set(4, 10, d);
    set(8, 8, a);
    set(8, 10, b);
    set(10, 10, c);
    k
}

/// Consistent nodal loads of a uniform load `q` (local axes, N/m) on a
/// frame member of length `l`, local axes.
pub(crate) fn frame_uniform_load(l: f64, q: Vector3<f64>) -> [f64; 12] {
    let h = l / 2.0;
    let m = l * l / 12.0;
    [
        q.x * h,
        q.y * h,
        q.z * h,
        0.0,
        -q.z * m,
        q.y * m,
        q.x * h,
        q.y * h,
        q.z * h,
        0.0,
        q.z * m,
        -q.y * m,
    ]
}

/// The 3 × 3 block of an axial member: EA/L·e·eᵀ + N/L·(I − e·eᵀ).
pub(crate) fn axial_block(e: &Vector3<f64>, ea_over_l: f64, n_over_l: f64) -> Matrix3<f64> {
    let eet = e * e.transpose();
    eet * ea_over_l + (Matrix3::identity() - eet) * n_over_l
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both matrices are symmetric; a rigid-body translation gives no
    /// force; the axes are orthonormal and right-handed.
    #[test]
    fn element_matrices_are_consistent() {
        let s = Section::elastic("s", 2e11, 1e-3, 2e-6);
        let k = frame_elastic(3.0, &s) + frame_geometric(3.0, -1e4);
        assert!((k - k.transpose()).abs().max() < 1e-6);
        let mut rigid = SMatrix::<f64, 12, 1>::zeros();
        for (i, v) in [1.0, -2.0, 0.5].iter().enumerate() {
            rigid[i] = *v;
            rigid[6 + i] = *v;
        }
        assert!((k * rigid).abs().max() < 1e-6);
        // Rigid rotation about z by θ: v = θ x, θz = θ.
        let mut rot = SMatrix::<f64, 12, 1>::zeros();
        rot[5] = 1e-3;
        rot[11] = 1e-3;
        rot[7] = 3.0 * 1e-3;
        assert!((frame_elastic(3.0, &s) * rot).abs().max() < 1e-6);
        let mut rot_y = SMatrix::<f64, 12, 1>::zeros();
        rot_y[4] = 1e-3;
        rot_y[10] = 1e-3;
        rot_y[8] = -3.0 * 1e-3;
        assert!((frame_elastic(3.0, &s) * rot_y).abs().max() < 1e-6);
        let model = Model {
            nodes: vec![
                crate::Node::free("a", [0.0, 0.0, 0.0]),
                crate::Node::free("b", [1.0, 2.0, 3.0]),
            ],
            ..Model::default()
        };
        let mut m = crate::Member::new("m", crate::MemberKind::Frame, 0, 1, 0);
        m.roll = 0.3;
        let r = axes(&model, &m);
        assert!((r * r.transpose() - Matrix3::identity()).abs().max() < 1e-12);
        assert!((r.determinant() - 1.0).abs() < 1e-12);
    }
}
