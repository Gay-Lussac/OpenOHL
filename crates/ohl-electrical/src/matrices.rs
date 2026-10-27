// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Per-unit-length matrices of a set of parallel wires: the primitive
//! series impedance and potential coefficients, Kron's reduction of the
//! earthed wires, bundles as one equivalent wire, symmetrical components.

use std::f64::consts::PI;

use nalgebra::{Complex, DMatrix};
use ohl_line::electrical::EarthReturn;

use crate::earth;

/// A complex matrix.
pub type CMatrix = DMatrix<Complex<f64>>;

/// One wire (or bundle, as its equivalent wire) of a cross-section, as the
/// matrices take it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wire {
    /// Across the line, m (any origin, the same for every wire).
    pub across: f64,
    /// Height above the earth, m.
    pub height: f64,
    /// Geometric mean radius, m (of the bundle: [`bundle`]).
    pub gmr: f64,
    /// Radius for the capacitance, m (of the bundle: [`bundle`]).
    pub radius: f64,
    /// AC resistance, Ω/m (of the bundle: the subconductors in parallel).
    pub resistance: f64,
}

/// The equivalent wire of a bundle of `n` subconductors on a circle with
/// `spacing` between neighbours: \\( (n\,g\,R^{n-1})^{1/n} \\) for a
/// subconductor's geometric mean radius (or radius) \\( g \\) and the
/// circle's radius \\( R = s/(2\sin(\pi/n)) \\) — the geometric mean of a
/// subconductor's distances to itself and to the others (their product,
/// on a regular polygon, is \\( nR^{n-1} \\)). Returns the equivalent of
/// `gmr` and of `radius`; one subconductor (or no spacing) as given.
pub fn bundle(n: u32, spacing: Option<f64>, gmr: f64, radius: f64) -> (f64, f64) {
    let Some(s) = spacing.filter(|s| n >= 2 && *s > 0.0) else {
        return (gmr, radius);
    };
    let nf = f64::from(n);
    let big_r = s / (2.0 * (PI / nf).sin());
    let product = nf * big_r.powi(n as i32 - 1);
    (
        (gmr * product).powf(1.0 / nf),
        (radius * product).powf(1.0 / nf),
    )
}

/// The primitive series impedance matrix, Ω/m, of `wires` at
/// `frequency_hz` over earth of resistivity `rho_ohm_m` ([`earth::impedance`]
/// plus each wire's resistance on the diagonal).
pub fn impedance(
    wires: &[Wire],
    method: EarthReturn,
    frequency_hz: f64,
    rho_ohm_m: f64,
) -> CMatrix {
    let n = wires.len();
    CMatrix::from_fn(n, n, |i, j| {
        let (a, b) = (&wires[i], &wires[j]);
        let d = if i == j {
            a.gmr
        } else {
            (a.across - b.across).hypot(a.height - b.height)
        };
        let z = earth::impedance(
            method,
            [a.across, a.height],
            [b.across, b.height],
            d,
            frequency_hz,
            rho_ohm_m,
        );
        if i == j {
            z + Complex::new(a.resistance, 0.0)
        } else {
            z
        }
    })
}

/// The potential coefficient matrix, m/F ([`earth::potential_coefficient`]).
pub fn potential(wires: &[Wire]) -> DMatrix<f64> {
    let n = wires.len();
    DMatrix::from_fn(n, n, |i, j| {
        let (a, b) = (&wires[i], &wires[j]);
        let d = if i == j {
            a.radius
        } else {
            (a.across - b.across).hypot(a.height - b.height)
        };
        earth::potential_coefficient([a.across, a.height], [b.across, b.height], d)
    })
}

/// Kron's reduction: the matrix of the wires `keep` with the others at
/// zero voltage (earthed), \\( M_{kk} - M_{ke} M_{ee}^{-1} M_{ek} \\).
/// `None` when the earthed block is singular.
pub fn kron(m: &CMatrix, keep: &[usize]) -> Option<CMatrix> {
    let n = m.nrows();
    let earthed: Vec<usize> = (0..n).filter(|i| !keep.contains(i)).collect();
    let pick = |rows: &[usize], cols: &[usize]| {
        CMatrix::from_fn(rows.len(), cols.len(), |i, j| m[(rows[i], cols[j])])
    };
    let mkk = pick(keep, keep);
    if earthed.is_empty() {
        return Some(mkk);
    }
    let inv = pick(&earthed, &earthed).try_inverse()?;
    Some(mkk - pick(keep, &earthed) * inv * pick(&earthed, keep))
}

/// A real matrix as a complex one.
pub fn complex(m: &DMatrix<f64>) -> CMatrix {
    m.map(|x| Complex::new(x, 0.0))
}

/// The symmetrical components' matrix of a three-phase matrix `m` with
/// phases in the order a, b, c (b lagging a by 120°):
/// \\( A^{-1} M A \\), \\( A = \begin{pmatrix}1&1&1\\\\1&\alpha^2&\alpha\\\\1&\alpha&\alpha^2\end{pmatrix} \\),
/// \\( \alpha = e^{j2\pi/3} \\) (Fortescue 1918); rows and columns 0
/// (zero), 1 (positive), 2 (negative sequence).
pub fn sequence(m: &CMatrix) -> CMatrix {
    let alpha = Complex::from_polar(1.0, 2.0 * PI / 3.0);
    let one = Complex::new(1.0, 0.0);
    let a = CMatrix::from_row_slice(
        3,
        3,
        &[
            one,
            one,
            one,
            one,
            alpha * alpha,
            alpha,
            one,
            alpha,
            alpha * alpha,
        ],
    );
    let a_inv = a.clone().map(|x| x.conj()) / Complex::new(3.0, 0.0);
    // A is symmetric and A⁻¹ = conj(A)/3.
    a_inv * m * a
}

/// The ideally transposed line's positive and zero sequence values of a
/// three-phase matrix: \\( m_s - m_m \\) and \\( m_s + 2m_m \\) with
/// \\( m_s \\) the mean of the diagonal, \\( m_m \\) of the rest.
pub fn transposed(m: &CMatrix) -> (Complex<f64>, Complex<f64>) {
    let n = m.nrows();
    let mut s = Complex::new(0.0, 0.0);
    let mut o = Complex::new(0.0, 0.0);
    for i in 0..n {
        for j in 0..n {
            if i == j {
                s += m[(i, j)];
            } else {
                o += m[(i, j)];
            }
        }
    }
    let nf = n as f64;
    let s = s / nf;
    let o = if n > 1 { o / (nf * (nf - 1.0)) } else { o };
    (s - o, s + o * (nf - 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(re: f64, im: f64) -> Complex<f64> {
        Complex::new(re, im)
    }

    /// Hand calculation: a symmetric matrix with self value s and mutual
    /// value m (every pair) has the sequence matrix diag(s + 2m, s − m,
    /// s − m) — the transposed line's — and no coupling between the
    /// sequences.
    #[test]
    fn sequence_of_a_balanced_matrix() {
        let (s, m) = (c(0.1, 0.4), c(0.05, 0.15));
        let mat = CMatrix::from_fn(3, 3, |i, j| if i == j { s } else { m });
        let z = sequence(&mat);
        assert!((z[(0, 0)] - (s + m * 2.0)).norm() < 1e-15);
        assert!((z[(1, 1)] - (s - m)).norm() < 1e-15);
        assert!((z[(2, 2)] - (s - m)).norm() < 1e-15);
        for (i, j) in [(0, 1), (0, 2), (1, 0), (1, 2), (2, 0), (2, 1)] {
            assert!(z[(i, j)].norm() < 1e-15);
        }
        let (z1, z0) = transposed(&mat);
        assert!((z1 - (s - m)).norm() < 1e-15 && (z0 - (s + m * 2.0)).norm() < 1e-15);
    }

    /// Hand calculation of an untransposed matrix (Fortescue's transform
    /// written out): with
    /// \\( Z = \begin{pmatrix}s&p&q\\\\p&s&p\\\\q&p&s\end{pmatrix} \\)
    /// (a flat line, the middle phase b),
    /// \\( Z_{00} = s + \tfrac{2}{3}(2p + q) \\),
    /// \\( Z_{11} = s - \tfrac{1}{3}(2p + q) \\) — the means of the
    /// transposed line; the coupling, row
    /// \\( \tfrac13(1, \alpha, \alpha^2) \\) of \\( A^{-1} \\) times
    /// \\( Z \\) times column \\( (1, \alpha, \alpha^2)^T \\) of \\( A \\),
    /// \\( Z_{12} = \tfrac13\left[s(1 + \alpha + \alpha^2) + 2p(1 + \alpha) + 2q\alpha^2\right]
    /// = \tfrac23\alpha^2(q - p) \\) (with \\( 1 + \alpha + \alpha^2 = 0 \\)):
    /// zero only when \\( p = q \\).
    #[test]
    fn sequence_of_a_flat_line() {
        let (s, p, q) = (c(0.30, 0.80), c(0.10, 0.40), c(0.10, 0.30));
        let mat = CMatrix::from_row_slice(3, 3, &[s, p, q, p, s, p, q, p, s]);
        let z = sequence(&mat);
        let third = 1.0 / 3.0;
        assert!((z[(0, 0)] - (s + (p * 2.0 + q) * (2.0 * third))).norm() < 1e-14);
        assert!((z[(1, 1)] - (s - (p * 2.0 + q) * third)).norm() < 1e-14);
        assert!((z[(2, 2)] - z[(1, 1)]).norm() < 1e-14);
        let alpha = Complex::from_polar(1.0, 2.0 * PI / 3.0);
        let z12 = alpha * alpha * (q - p) * (2.0 * third);
        assert!((z[(1, 2)] - z12).norm() < 1e-14, "{} {}", z[(1, 2)], z12);
        let (z1, z0) = transposed(&mat);
        assert!((z1 - z[(1, 1)]).norm() < 1e-14 && (z0 - z[(0, 0)]).norm() < 1e-14);
    }

    /// Kron's reduction by hand of a 2×2 matrix: \\( m_{00} - m_{01}m_{10}/m_{11} \\).
    #[test]
    fn kron_by_hand() {
        let m =
            CMatrix::from_row_slice(2, 2, &[c(2.0, 1.0), c(0.5, 0.2), c(0.5, 0.2), c(1.5, 0.7)]);
        let r = kron(&m, &[0]).unwrap();
        let hand = c(2.0, 1.0) - c(0.5, 0.2) * c(0.5, 0.2) / c(1.5, 0.7);
        assert!((r[(0, 0)] - hand).norm() < 1e-15);
    }

    /// Bundle of three on a triangle of side 0.3 m, subconductor GMR
    /// 0.78 × 0.015 m: \\( (0.0117 \cdot 0.3^2)^{1/3} = 0.1018 \\) m —
    /// the equilateral triangle's distances, written out.
    #[test]
    fn bundle_of_three_by_hand() {
        let (g, r) = bundle(3, Some(0.3), 0.78 * 0.015, 0.015);
        assert!((g - (0.78 * 0.015 * 0.09_f64).cbrt()).abs() < 1e-12);
        assert!((r - (0.015 * 0.09_f64).cbrt()).abs() < 1e-12);
        // Four on a square of side s: distances s, s, s√2.
        let (g4, _) = bundle(4, Some(0.4), 0.01, 0.015);
        let hand = (0.01 * 0.4 * 0.4 * 0.4 * 2f64.sqrt()).powf(0.25);
        assert!((g4 - hand).abs() < 1e-12);
    }
}
