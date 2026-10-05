// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Symmetric skyline (profile) matrix and its LDLᵀ factorisation, the
//! active-column solver of Bathe ("Finite Element Procedures", §8.2.3),
//! with a reverse Cuthill–McKee node order to keep the profile small.
//! Cost: about n·b²/2 operations for n equations of mean column height b.

/// Symmetric matrix stored by columns, from the first non-zero row down
/// to the diagonal.
#[derive(Debug, Clone)]
pub(crate) struct Skyline {
    /// First row stored of each column.
    first: Vec<usize>,
    /// Start of each column in `values`; one more entry at the end.
    start: Vec<usize>,
    values: Vec<f64>,
}

/// Why a factorisation failed.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Pivot {
    /// A pivot (nearly) zero: equation index.
    Zero(usize),
}

impl Skyline {
    /// An empty matrix with the profile given by each column's first row.
    pub fn new(first: Vec<usize>) -> Self {
        let mut start = Vec::with_capacity(first.len() + 1);
        let mut total = 0;
        for (j, f) in first.iter().enumerate() {
            debug_assert!(*f <= j);
            start.push(total);
            total += j - f + 1;
        }
        start.push(total);
        Self {
            first,
            start,
            values: vec![0.0; total],
        }
    }

    /// Number of equations.
    pub fn len(&self) -> usize {
        self.first.len()
    }

    /// Sets every entry to 0 (keeps the profile).
    pub fn clear(&mut self) {
        self.values.iter_mut().for_each(|v| *v = 0.0);
    }

    #[inline]
    fn at(&self, i: usize, j: usize) -> usize {
        self.start[j] + i - self.first[j]
    }

    /// Adds `v` at (i, j) (and so at (j, i)); outside the profile panics.
    #[inline]
    pub fn add(&mut self, i: usize, j: usize, v: f64) {
        let (i, j) = if i <= j { (i, j) } else { (j, i) };
        assert!(i >= self.first[j], "entry outside the profile");
        let k = self.at(i, j);
        self.values[k] += v;
    }

    /// y = A·x.
    pub fn mul(&self, x: &[f64]) -> Vec<f64> {
        let mut y = vec![0.0; x.len()];
        for j in 0..self.len() {
            let f = self.first[j];
            let col = &self.values[self.start[j]..self.start[j + 1]];
            for (r, a) in col.iter().enumerate() {
                let i = f + r;
                y[i] += a * x[j];
                if i != j {
                    y[j] += a * x[i];
                }
            }
        }
        y
    }

    /// Factorises in place into L·D·Lᵀ (unit L above the diagonal, D on
    /// it). Returns the number of negative pivots (eigenvalues below 0:
    /// beyond a buckling load). A pivot below `1e-12` times its diagonal
    /// entry is a mechanism.
    pub fn factorize(&mut self) -> Result<usize, Pivot> {
        let n = self.len();
        let mut negative = 0;
        for j in 0..n {
            let fj = self.first[j];
            let sj = self.start[j];
            let diag = self.values[self.at(j, j)];
            // g_ij = a_ij − Σ l_ri g_rj, rows i of the column above j.
            for i in fj..j {
                let fi = self.first[i];
                let m = fi.max(fj);
                let si = self.start[i];
                let mut sum = 0.0;
                for r in m..i {
                    sum += self.values[si + r - fi] * self.values[sj + r - fj];
                }
                self.values[sj + i - fj] -= sum;
            }
            // l_ij = g_ij / d_i, d_j = a_jj − Σ l_ij g_ij.
            let mut d = diag;
            for i in fj..j {
                let g = self.values[sj + i - fj];
                let l = g / self.values[self.at(i, i)];
                d -= l * g;
                self.values[sj + i - fj] = l;
            }
            if d.is_nan() || d.abs() <= 1e-12 * diag.abs().max(f64::MIN_POSITIVE) || !d.is_finite()
            {
                return Err(Pivot::Zero(j));
            }
            if d < 0.0 {
                negative += 1;
            }
            let k = self.at(j, j);
            self.values[k] = d;
        }
        Ok(negative)
    }

    /// Solves with a factorised matrix, `b` replaced by the solution.
    pub fn solve(&self, b: &mut [f64]) {
        let n = self.len();
        // L·y = b.
        for j in 0..n {
            let fj = self.first[j];
            let sj = self.start[j];
            let sum: f64 = self.values[sj..sj + j - fj]
                .iter()
                .zip(&b[fj..j])
                .map(|(l, y)| l * y)
                .sum();
            b[j] -= sum;
        }
        // D·z = y.
        for (j, x) in b.iter_mut().enumerate() {
            *x /= self.values[self.at(j, j)];
        }
        // Lᵀ·x = z.
        for j in (0..n).rev() {
            let fj = self.first[j];
            let sj = self.start[j];
            let xj = b[j];
            for (x, l) in b[fj..j].iter_mut().zip(&self.values[sj..sj + j - fj]) {
                *x -= l * xj;
            }
        }
    }
}

/// Reverse Cuthill–McKee order of the nodes of a graph (`adjacent[n]`:
/// the neighbours of node n): `order[k]` is the node numbered k.
pub(crate) fn reverse_cuthill_mckee(adjacent: &[Vec<usize>]) -> Vec<usize> {
    let n = adjacent.len();
    let degree = |k: usize| adjacent[k].len();
    let mut order = Vec::with_capacity(n);
    let mut placed = vec![false; n];
    let mut by_degree: Vec<usize> = (0..n).collect();
    by_degree.sort_by_key(|&k| degree(k));
    for &seed in &by_degree {
        if placed[seed] {
            continue;
        }
        // Each connected part from a node of least degree.
        placed[seed] = true;
        let mut head = order.len();
        order.push(seed);
        while head < order.len() {
            let k = order[head];
            head += 1;
            let mut next: Vec<usize> = adjacent[k]
                .iter()
                .copied()
                .filter(|&m| !placed[m])
                .collect();
            next.sort_by_key(|&m| degree(m));
            next.dedup();
            for m in next {
                if !placed[m] {
                    placed[m] = true;
                    order.push(m);
                }
            }
        }
    }
    order.reverse();
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small symmetric (indefinite) matrix solved against a dense check.
    #[test]
    fn solves_symmetric_systems() {
        // Profile: column 0 [0], 1 [0..1], 2 [1..2], 3 [0..3].
        let mut a = Skyline::new(vec![0, 0, 1, 0]);
        let dense = [
            [4.0, 1.0, 0.0, 0.5],
            [1.0, -3.0, 2.0, 0.0],
            [0.0, 2.0, 5.0, 1.0],
            [0.5, 0.0, 1.0, 2.0],
        ];
        for (i, row) in dense.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                if i <= j && *v != 0.0 {
                    a.add(i, j, *v);
                }
            }
        }
        let x = [1.0, -2.0, 0.5, 3.0];
        let b = a.mul(&x);
        let mut f = a.clone();
        let negative = f.factorize().unwrap();
        assert_eq!(negative, 1);
        let mut y = b.clone();
        f.solve(&mut y);
        for (p, q) in y.iter().zip(&x) {
            assert!((p - q).abs() < 1e-12, "{y:?}");
        }
        // A singular matrix is a mechanism.
        let mut s = Skyline::new(vec![0, 0]);
        s.add(0, 0, 1.0);
        s.add(0, 1, 1.0);
        s.add(1, 1, 1.0);
        assert_eq!(s.factorize(), Err(Pivot::Zero(1)));
    }

    #[test]
    fn rcm_orders_every_node() {
        // A path 0-3-1-4-2 plus an isolated node 5.
        let adj = vec![vec![3], vec![3, 4], vec![4], vec![0, 1], vec![1, 2], vec![]];
        let mut o = reverse_cuthill_mckee(&adj);
        assert_eq!(o.len(), 6);
        // Neighbours along the path end up next to each other.
        let pos = |k: usize| o.iter().position(|&x| x == k).unwrap() as i64;
        assert!((pos(0) - pos(3)).abs() == 1 && (pos(3) - pos(1)).abs() == 1);
        o.sort();
        assert_eq!(o, vec![0, 1, 2, 3, 4, 5]);
    }
}
