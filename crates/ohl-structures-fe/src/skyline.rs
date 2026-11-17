// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Symmetric skyline (profile) matrix and its LDLᵀ factorisation, the
//! active-column solver of Bathe ("Finite Element Procedures", §8.2.3),
//! with a reverse Cuthill–McKee node order to keep the profile small.
//! Cost: about n·b²/2 operations for n equations of mean column height b.
//!
//! **Speed without a changed digit** (2026-11-17, calculation level 4's
//! speed-up): every sum is formed as the textbook loop forms it — the same
//! terms, added one after the other in the same order, from the same
//! start — but several independent sums side by side (the rows of a
//! column in the factorisation, the right-hand sides of
//! [`Skyline::solve_many`]): one sum is a chain of additions each waiting
//! for the last, several keep the processor's units busy. Rust does no
//! fast-math (no reordering, no fused multiply-add), so the results are
//! the textbook loop's bit for bit; the tests compare them with it
//! (`kernels_are_the_textbook_loops_bit_for_bit`).

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

/// Right-hand sides whose sums a solve forms side by side.
const BLOCK: usize = 4;

/// Rows of a column whose sums the factorisation forms side by side.
const ROWS: usize = 8;

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
    ///
    /// Each y_i is summed as the column-by-column loop sums it (column j
    /// adds a_ij·x_j to the rows above its diagonal, and Σ a_ij·x_i, i
    /// ascending, then a_jj·x_j, to y_j), y_j's own sum kept in a
    /// register.
    pub fn mul(&self, x: &[f64]) -> Vec<f64> {
        let mut y = vec![0.0; x.len()];
        for j in 0..self.len() {
            let f = self.first[j];
            let col = &self.values[self.start[j]..self.start[j + 1]];
            let (above, diagonal) = col.split_at(col.len() - 1);
            let xj = x[j];
            let mut yj = y[j];
            for ((yi, xi), a) in y[f..j].iter_mut().zip(&x[f..j]).zip(above) {
                *yi += a * xj;
                yj += a * xi;
            }
            y[j] = yj + diagonal[0] * xj;
        }
        y
    }

    /// Row q of A·x ([`Skyline::mul`]'s y_q: the same sum in the same
    /// order) from the entries `row` of [`Skyline::row`]`(q)`.
    pub fn mul_row(&self, row: &[(usize, usize)], x: &[f64]) -> f64 {
        let mut y = 0.0;
        for &(k, at) in row {
            y += self.values[at] * x[k];
        }
        y
    }

    /// The entries of row `q` in the order [`Skyline::mul`] adds them to
    /// y_q: per entry, the equation it multiplies and its place in the
    /// storage — column q from its first row down to the diagonal, then
    /// the columns right of it that reach row q.
    pub fn row(&self, q: usize) -> Vec<(usize, usize)> {
        let f = self.first[q];
        let mut row: Vec<(usize, usize)> = (f..=q).map(|i| (i, self.at(i, q))).collect();
        for j in q + 1..self.len() {
            if self.first[j] <= q {
                row.push((j, self.at(q, j)));
            }
        }
        row
    }

    /// Factorises in place into L·D·Lᵀ (unit L above the diagonal, D on
    /// it). Returns the number of negative pivots (eigenvalues below 0:
    /// beyond a buckling load). A pivot below `1e-12` times its diagonal
    /// entry is a mechanism.
    pub fn factorize(&mut self) -> Result<usize, Pivot> {
        self.factor(None)
    }

    /// [`Skyline::factorize`]; with `stand_in`, a zero pivot is replaced
    /// by it and the factorisation goes on.
    fn factor(&mut self, stand_in: Option<f64>) -> Result<usize, Pivot> {
        let n = self.len();
        let mut negative = 0;
        for j in 0..n {
            let fj = self.first[j];
            let sj = self.start[j];
            let diag = self.values[self.at(j, j)];
            // g_ij = a_ij − Σ l_ri g_rj, rows i of the column above j.
            self.reduce_column(j);
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
                match stand_in {
                    Some(s) => d = s,
                    None => return Err(Pivot::Zero(j)),
                }
            }
            if d < 0.0 {
                negative += 1;
            }
            let k = self.at(j, j);
            self.values[k] = d;
        }
        Ok(negative)
    }

    /// The motion of the mechanisms of a singular matrix (not factorised):
    /// a vector K hardly resists, from two steps of inverse iteration on
    /// the matrix with its zero pivots replaced by 10⁻¹² of its largest
    /// diagonal entry — the zero-energy motions (every one of them, the
    /// whole part that moves) grow by about the ratio of the stiffness to
    /// that, the others do not. Replaces the matrix by its factors.
    pub fn mechanism(&mut self) -> Vec<f64> {
        let n = self.len();
        let largest = (0..n)
            .map(|j| self.values[self.at(j, j)].abs())
            .fold(0.0, f64::max)
            .max(f64::MIN_POSITIVE);
        let _ = self.factor(Some(1e-12 * largest));
        // A start no motion is orthogonal to (deterministic).
        let mut x: Vec<f64> = (0..n)
            .map(|i| 1.0 + ((i * 7919) % 13) as f64 / 13.0)
            .collect();
        for _ in 0..2 {
            self.solve(&mut x);
            let size = x.iter().fold(0.0f64, |a, v| a.max(v.abs()));
            if size.is_finite() && size > 0.0 {
                x.iter_mut().for_each(|v| *v /= size);
            }
        }
        x
    }

    /// g_ij = a_ij − Σ_r l_ri·g_rj for the rows i above the diagonal of
    /// column j (the columns left of it factorised), each sum over r
    /// ascending from the first row both columns hold, from 0 — the
    /// textbook loop's sums, [`ROWS`] rows side by side: their terms on
    /// the rows above the block, whose g are known, summed together, then
    /// each row's last terms (on the block's rows above it) in turn as
    /// their g are found.
    fn reduce_column(&mut self, j: usize) {
        let fj = self.first[j];
        let sj = self.start[j];
        let first = &self.first;
        let start = &self.start;
        // The columns left of j (factorised), and column j (col[r − fj]:
        // its row r).
        let (left, rest) = self.values.split_at_mut(sj);
        let left: &[f64] = left;
        let col = &mut rest[..j + 1 - fj];
        // Column i's entry at row r.
        let l = |i: usize, r: usize| left[start[i] + r - first[i]];
        let mut i0 = fj;
        while i0 + ROWS <= j {
            // Each row's first term; the rows all of them hold from `lo`.
            let m: [usize; ROWS] = std::array::from_fn(|b| first[i0 + b].max(fj));
            let lo = m.iter().copied().max().unwrap_or(i0).min(i0);
            let mut s = [0.0_f64; ROWS];
            // Each row's terms above `lo` (rows only some of them hold).
            for b in 0..ROWS {
                for r in m[b]..lo {
                    s[b] += l(i0 + b, r) * col[r - fj];
                }
            }
            // The rows lo..i0, together.
            if lo < i0 {
                let g = &col[lo - fj..i0 - fj];
                let n = g.len();
                let c: [&[f64]; ROWS] = std::array::from_fn(|b| {
                    let i = i0 + b;
                    &left[start[i] + lo - first[i]..][..n]
                });
                for (k, gk) in g.iter().enumerate() {
                    for b in 0..ROWS {
                        s[b] += c[b][k] * gk;
                    }
                }
            }
            // The block's own rows, each g as it is found.
            for (b, sum) in s.iter_mut().enumerate() {
                let i = i0 + b;
                for r in m[b].max(i0)..i {
                    *sum += l(i, r) * col[r - fj];
                }
                col[i - fj] -= *sum;
            }
            i0 += ROWS;
        }
        // The rows left over, one by one.
        for i in i0..j {
            let mut sum = 0.0;
            for r in first[i].max(fj)..i {
                sum += l(i, r) * col[r - fj];
            }
            col[i - fj] -= sum;
        }
    }

    /// Solves with a factorised matrix, `b` replaced by the solution.
    pub fn solve(&self, b: &mut [f64]) {
        self.solve_many(&mut [b]);
    }

    /// [`Skyline::solve`] of each of `bs` (each replaced by its solution),
    /// the right-hand sides side by side: each solved by the same
    /// operations in the same order as alone.
    pub fn solve_many(&self, bs: &mut [&mut [f64]]) {
        let n = self.len();
        // A sum's start as `Iterator::sum` starts it (the forward
        // substitution's sums were `sum()`s).
        let zero: f64 = std::iter::empty::<f64>().sum();
        // L·y = b.
        for j in 0..n {
            let fj = self.first[j];
            let sj = self.start[j];
            let lj = &self.values[sj..sj + j - fj];
            let mut chunks = bs.chunks_exact_mut(BLOCK);
            for four in &mut chunks {
                let [b0, b1, b2, b3] = four else {
                    unreachable!("chunks of BLOCK")
                };
                let (mut s0, mut s1, mut s2, mut s3) = (zero, zero, zero, zero);
                let (y0, y1, y2, y3) = (&b0[fj..j], &b1[fj..j], &b2[fj..j], &b3[fj..j]);
                for (k, l) in lj.iter().enumerate() {
                    s0 += l * y0[k];
                    s1 += l * y1[k];
                    s2 += l * y2[k];
                    s3 += l * y3[k];
                }
                b0[j] -= s0;
                b1[j] -= s1;
                b2[j] -= s2;
                b3[j] -= s3;
            }
            for b in chunks.into_remainder() {
                let mut sum = zero;
                for (l, y) in lj.iter().zip(&b[fj..j]) {
                    sum += l * y;
                }
                b[j] -= sum;
            }
        }
        for b in bs.iter_mut() {
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

    /// The factorisation as it was written before 2026-11-17: one sum at
    /// a time (the reference the kernels are compared with).
    fn textbook_factor(k: &mut Skyline) -> Result<usize, Pivot> {
        let n = k.len();
        let mut negative = 0;
        for j in 0..n {
            let fj = k.first[j];
            let sj = k.start[j];
            let diag = k.values[k.at(j, j)];
            for i in fj..j {
                let fi = k.first[i];
                let m = fi.max(fj);
                let si = k.start[i];
                let mut sum = 0.0;
                for r in m..i {
                    sum += k.values[si + r - fi] * k.values[sj + r - fj];
                }
                k.values[sj + i - fj] -= sum;
            }
            let mut d = diag;
            for i in fj..j {
                let g = k.values[sj + i - fj];
                let l = g / k.values[k.at(i, i)];
                d -= l * g;
                k.values[sj + i - fj] = l;
            }
            if d.is_nan() || d.abs() <= 1e-12 * diag.abs().max(f64::MIN_POSITIVE) || !d.is_finite()
            {
                return Err(Pivot::Zero(j));
            }
            if d < 0.0 {
                negative += 1;
            }
            let at = k.at(j, j);
            k.values[at] = d;
        }
        Ok(negative)
    }

    /// The substitutions as written before 2026-11-17.
    fn textbook_solve(k: &Skyline, b: &mut [f64]) {
        let n = k.len();
        for j in 0..n {
            let fj = k.first[j];
            let sj = k.start[j];
            let sum: f64 = k.values[sj..sj + j - fj]
                .iter()
                .zip(&b[fj..j])
                .map(|(l, y)| l * y)
                .sum();
            b[j] -= sum;
        }
        for (j, x) in b.iter_mut().enumerate() {
            *x /= k.values[k.at(j, j)];
        }
        for j in (0..n).rev() {
            let fj = k.first[j];
            let sj = k.start[j];
            let xj = b[j];
            for (x, l) in b[fj..j].iter_mut().zip(&k.values[sj..sj + j - fj]) {
                *x -= l * xj;
            }
        }
    }

    /// The product as written before 2026-11-17.
    fn textbook_mul(k: &Skyline, x: &[f64]) -> Vec<f64> {
        let mut y = vec![0.0; x.len()];
        for j in 0..k.len() {
            let f = k.first[j];
            let col = &k.values[k.start[j]..k.start[j + 1]];
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

    fn bits(v: &[f64]) -> Vec<u64> {
        v.iter().map(|x| x.to_bits()).collect()
    }

    /// A deterministic sequence in [−1, 1).
    struct Numbers(u64);
    impl Numbers {
        fn next(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 52) as f64 - 1.0
        }
    }

    /// Profiles of every shape (columns of one entry, ragged first rows,
    /// a full band, blocks left over), definite, indefinite and singular
    /// matrices, signed zeros among the entries: the kernels give the
    /// textbook loops' factors, solutions and products bit for bit.
    #[test]
    fn kernels_are_the_textbook_loops_bit_for_bit() {
        let mut x = Numbers(0x9E37_79B9_7F4A_7C15);
        for case in 0..400 {
            let n: usize = 1 + case % 37 + (case / 37) * 7;
            let band = 1 + case % 23;
            let first: Vec<usize> = (0..n)
                .map(|j| {
                    let reach = ((x.next() + 1.0) * band as f64) as usize;
                    j.saturating_sub(reach)
                })
                .collect();
            let mut k = Skyline::new(first.clone());
            for j in 0..n {
                for i in first[j]..=j {
                    let v = if i == j {
                        // Mostly dominant, some negative or small.
                        match case % 5 {
                            0 => x.next(),
                            1 => -0.0,
                            _ => 2.0 * band as f64 + x.next(),
                        }
                    } else if (i + j + case) % 11 == 0 {
                        -0.0
                    } else {
                        x.next()
                    };
                    let at = k.at(i, j);
                    k.values[at] = v;
                }
            }
            let v: Vec<f64> = (0..n).map(|_| x.next()).collect();
            assert_eq!(bits(&k.mul(&v)), bits(&textbook_mul(&k, &v)), "case {case}");
            for q in 0..n {
                assert_eq!(
                    k.mul_row(&k.row(q), &v).to_bits(),
                    textbook_mul(&k, &v)[q].to_bits(),
                    "case {case} row {q}"
                );
            }
            let mut a = k.clone();
            let mut b = k.clone();
            let ra = a.factorize();
            let rb = textbook_factor(&mut b);
            assert_eq!(ra, rb, "case {case}");
            if ra.is_err() {
                continue;
            }
            assert_eq!(bits(&a.values), bits(&b.values), "case {case}");
            let rhs: Vec<Vec<f64>> = (0..(case % 9))
                .map(|_| (0..n).map(|_| x.next()).collect())
                .collect();
            let mut many = rhs.clone();
            let mut slices: Vec<&mut [f64]> = many.iter_mut().map(|v| v.as_mut_slice()).collect();
            a.solve_many(&mut slices);
            for (r, got) in rhs.iter().zip(&many) {
                let mut want = r.clone();
                textbook_solve(&b, &mut want);
                assert_eq!(bits(got), bits(&want), "case {case}");
                let mut one = r.clone();
                a.solve(&mut one);
                assert_eq!(bits(&one), bits(&want), "case {case}");
            }
        }
    }

    /// The kernels' time against the textbook loops on a matrix the size
    /// of a large lattice tower's (4 000 equations, columns about 300
    /// high): `cargo test --profile bench-fast -p ohl-structures-fe --lib
    /// kernel_speed -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing"]
    fn kernel_speed() {
        let mut x = Numbers(0x2545_F491_4F6C_DD1D);
        let n = 4000;
        let first: Vec<usize> = (0..n)
            .map(|j: usize| j.saturating_sub(250 + ((x.next() + 1.0) * 50.0) as usize))
            .collect();
        let mut k = Skyline::new(first.clone());
        for j in 0..n {
            for i in first[j]..=j {
                let v = if i == j { 1000.0 } else { x.next() };
                k.add(i, j, v);
            }
        }
        let time = |f: &mut dyn FnMut()| {
            let mut best = f64::INFINITY;
            for _ in 0..3 {
                let t = std::time::Instant::now();
                f();
                best = best.min(t.elapsed().as_secs_f64());
            }
            best
        };
        let (mut a, mut b) = (k.clone(), k.clone());
        let t_new = time(&mut || {
            a = k.clone();
            a.factorize().unwrap();
        });
        let t_old = time(&mut || {
            b = k.clone();
            textbook_factor(&mut b).unwrap();
        });
        assert_eq!(bits(&a.values), bits(&b.values));
        let v: Vec<f64> = (0..n).map(|_| x.next()).collect();
        let m_new = time(&mut || {
            std::hint::black_box(k.mul(&v));
        });
        let m_old = time(&mut || {
            std::hint::black_box(textbook_mul(&k, &v));
        });
        let rhs: Vec<Vec<f64>> = (0..30).map(|_| v.clone()).collect();
        let s_new = time(&mut || {
            let mut many = rhs.clone();
            let mut s: Vec<&mut [f64]> = many.iter_mut().map(|v| v.as_mut_slice()).collect();
            a.solve_many(&mut s);
        });
        let s_one = time(&mut || {
            let mut many = rhs.clone();
            for r in &mut many {
                a.solve(r);
            }
        });
        let s_old = time(&mut || {
            let mut many = rhs.clone();
            for r in &mut many {
                textbook_solve(&b, r);
            }
        });
        println!(
            "factor {:.1} ms (textbook {:.1}); mul {:.2} ms ({:.2}); 30 solves together {:.1} ms, \
             one by one {:.1} ms (textbook {:.1})",
            t_new * 1e3,
            t_old * 1e3,
            m_new * 1e3,
            m_old * 1e3,
            s_new * 1e3,
            s_one * 1e3,
            s_old * 1e3
        );
    }

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
        // Its motion: both equations together, along (−1, 1).
        let mut s = Skyline::new(vec![0, 0]);
        s.add(0, 0, 1.0);
        s.add(0, 1, 1.0);
        s.add(1, 1, 1.0);
        let v = s.mechanism();
        assert!((v[0] + v[1]).abs() < 1e-9 * v[1].abs(), "{v:?}");
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
