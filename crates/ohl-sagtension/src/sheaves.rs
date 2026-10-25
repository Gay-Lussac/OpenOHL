// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A section in the blocks over frictionless sheaves (owner decision
//! 2026-10-25): the wire runs over a sheave at every suspension clamp, so
//! its **tension along the wire** is the same on both sides of each sheave
//! (not its horizontal tension). On an inclined span the two differ by
//! about the load per metre times the difference of the end's heights
//! above the two spans' low points (for a catenary of load \\( w \\) per
//! metre of wire, \\( T = H + w\\,(z - z_{low}) \\) along it).
//!
//! [`share`] finds the spans' horizontal tensions \\( H_1 \dots H_n \\) so
//! that
//!
//! \\[ T_{end,i}(H_i) = T_{start,i+1}(H_{i+1}), \qquad
//!     \sum_i L_{0,i}(H_i) = L_0 \\]
//!
//! with one unknown, the first span's \\( H_1 \\): each next span's
//! \\( H \\) follows from the equal tension at the sheave before it (a
//! bracketed secant per span), and the section's unstressed length is the
//! residual (a bracketed secant on \\( H_1 \\): every \\( H \\) rises with
//! \\( H_1 \\), so the length needed falls). The sheave's size is left
//! out (the wire runs over it at the clamp point) and so is its friction.
//! The geometry of each span (its chord, its load) is the caller's: the
//! 2D sections of this crate and the finite-element sections of the line
//! use it alike.

use ohl_core::budget::{Budget, Stop};

/// One span at a trial horizontal tension.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SheaveSpan {
    /// Its unstressed length (in the section's reference), m.
    pub unstressed: f64,
    /// The wire's tension at its back end (the sheave before it), N.
    pub tension_start: f64,
    /// The wire's tension at its ahead end, N.
    pub tension_end: f64,
}

/// Why the sharing over the sheaves failed.
#[derive(Debug, Clone, PartialEq)]
pub enum ShareError<E> {
    /// A span's own solve failed.
    Span(E),
    /// The iteration ran out of its budget (outer: the first span's
    /// tension) or of its exit clause (inner: one span's).
    Stopped {
        /// Why.
        stop: Stop,
        /// Where it was.
        detail: String,
    },
    /// No tension shares the length (a span too slack to compute at any
    /// tension the bracket reached).
    NoSolution(String),
}

impl<E: std::fmt::Display> std::fmt::Display for ShareError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Span(e) => write!(f, "{e}"),
            Self::Stopped { stop, detail } => {
                write!(
                    f,
                    "sharing over the sheaves did not converge: {stop}; {detail}"
                )
            }
            Self::NoSolution(m) => write!(f, "sharing over the sheaves: {m}"),
        }
    }
}

/// Steps of one span's search for the tension at its back sheave (exit
/// clause; a bracketed secant converges in 5–10).
pub const SPAN_STEPS: u64 = 200;

/// The horizontal tensions of `n` spans in the blocks over frictionless
/// sheaves sharing the unstressed length `total` (module docs), from
/// `guess` (N, e.g. the equal-H tension). `span(i, h)` is span `i` at
/// horizontal tension `h`; `None` when too slack to compute (taken as
/// needing more wire than any taut tension).
///
/// Expected cost: ~10–20 outer steps × n spans × ~5–10 span solves.
/// Exit clauses: `budget` on the outer steps, [`SPAN_STEPS`] per span.
pub fn share<E>(
    n: usize,
    total: f64,
    guess: f64,
    mut span: impl FnMut(usize, f64) -> Result<Option<SheaveSpan>, E>,
    budget: &Budget,
) -> Result<Vec<f64>, ShareError<E>> {
    if n == 0 || total.is_nan() || total <= 0.0 || guess.is_nan() || guess <= 0.0 {
        return Err(ShareError::NoSolution(format!(
            "{n} spans, length {total} m, first tension {guess} N"
        )));
    }
    // The spans' tensions and the length they need, from the first's;
    // None: some span too slack (the first span's tension too low).
    let mut propagate = |h1: f64| -> Result<Option<(Vec<f64>, f64)>, ShareError<E>> {
        let Some(first) = span(0, h1).map_err(ShareError::Span)? else {
            return Ok(None);
        };
        let mut hs = vec![h1];
        let mut length = first.unstressed;
        let mut t = first.tension_end;
        for i in 1..n {
            let Some((h, s)) = at_sheave(&mut span, i, t, hs[i - 1])? else {
                return Ok(None);
            };
            hs.push(h);
            length += s.unstressed;
            t = s.tension_end;
        }
        Ok(Some((hs, length)))
    };
    // Excess length over the section's: falls as the first tension rises;
    // +∞ when too slack.
    let mut excess = |h: f64| -> Result<(f64, Option<Vec<f64>>), ShareError<E>> {
        Ok(match propagate(h)? {
            Some((hs, l)) => (l - total, Some(hs)),
            None => (f64::INFINITY, None),
        })
    };
    let mut run = budget.start();
    let stopped = |stop: Stop, lo: f64, hi: f64| ShareError::Stopped {
        stop,
        detail: format!("first span's tension in [{lo:.6e}, {hi:.6e}] N"),
    };
    // The sheaves move the tensions by tenths of a percent from equal H.
    let (mut lo, mut hi) = (guess / 1.01, guess * 1.01);
    let (mut f_lo, _) = excess(lo)?;
    while f_lo < 0.0 {
        run.step().map_err(|s| stopped(s, lo, hi))?;
        (hi, lo) = (lo, lo / 2.0);
        f_lo = excess(lo)?.0;
    }
    let (mut f_hi, mut found) = excess(hi)?;
    while f_hi > 0.0 {
        run.step().map_err(|s| stopped(s, lo, hi))?;
        (lo, f_lo, hi) = (hi, f_hi, hi * 2.0);
        (f_hi, found) = excess(hi)?;
    }
    // Bracketed secant (Illinois), halving while one side is too slack.
    let mut side = 0;
    while hi - lo > 1e-12 * hi {
        run.step().map_err(|s| stopped(s, lo, hi))?;
        let mut h = if f_lo.is_finite() {
            (lo * f_hi - hi * f_lo) / (f_hi - f_lo)
        } else {
            0.5 * (lo + hi)
        };
        if !(lo < h && h < hi) {
            h = 0.5 * (lo + hi);
        }
        let (f, hs) = excess(h)?;
        if hs.is_some() && f.abs() <= 1e-13 * total {
            found = hs;
            break;
        }
        if f > 0.0 {
            (lo, f_lo) = (h, f);
            if side == 1 {
                f_hi /= 2.0;
            }
            side = 1;
        } else {
            (hi, f_hi, found) = (h, f, hs);
            if side == -1 && f_lo.is_finite() {
                f_lo /= 2.0;
            }
            side = -1;
        }
    }
    found.ok_or_else(|| {
        ShareError::NoSolution(format!(
            "no taut tension found (first span's tension near {hi:.6e} N)"
        ))
    })
}

/// Span `i`'s horizontal tension giving tension `t` at its back sheave
/// (the wire's tension there equal to the span before's), from `guess`;
/// with the span there. None: too slack at every tension up to `t`.
///
/// The wire's tension at an end is at least its horizontal tension, so
/// the root lies at or below `t`; below the root the end's tension is
/// lower (the taut branch, where it rises with \\( H \\)). Bracketed
/// secant, exit clause [`SPAN_STEPS`].
fn at_sheave<E>(
    span: &mut impl FnMut(usize, f64) -> Result<Option<SheaveSpan>, E>,
    i: usize,
    t: f64,
    guess: f64,
) -> Result<Option<(f64, SheaveSpan)>, ShareError<E>> {
    let mut steps = 0;
    let mut step = |lo: f64, hi: f64| {
        steps += 1;
        if steps > SPAN_STEPS {
            Err(ShareError::Stopped {
                stop: Stop::IterationLimit {
                    iterations: SPAN_STEPS,
                    elapsed: std::time::Duration::ZERO,
                },
                detail: format!(
                    "span {}: its tension at the sheave in [{lo:.6e}, {hi:.6e}] N, wanted {t:.6e} N",
                    i + 1
                ),
            })
        } else {
            Ok(())
        }
    };
    let mut at = |h: f64| -> Result<Option<SheaveSpan>, ShareError<E>> {
        span(i, h).map_err(ShareError::Span)
    };
    // The upper end: H = t (the end's tension is no lower than H).
    let mut hi = t;
    let Some(s_hi) = at(hi)? else {
        return Ok(None);
    };
    let mut f_hi = s_hi.tension_start - t;
    if f_hi <= 0.0 {
        return Ok(Some((hi, s_hi)));
    }
    let mut best = (hi, s_hi);
    // The lower end: from the guess (the span before's H), halving.
    let mut lo = guess.min(t * (1.0 - 1e-6));
    let mut f_lo;
    loop {
        step(lo, hi)?;
        match at(lo)? {
            Some(s) if s.tension_start < t => {
                f_lo = s.tension_start - t;
                break;
            }
            Some(s) => {
                (hi, f_hi, best) = (lo, s.tension_start - t, (lo, s));
                lo /= 2.0;
            }
            None => {
                f_lo = f64::NEG_INFINITY;
                break;
            }
        }
        if lo < 1e-9 * t {
            return Ok(None);
        }
    }
    let mut side = 0;
    while hi - lo > 1e-13 * hi {
        step(lo, hi)?;
        let mut h = if f_lo.is_finite() {
            (lo * f_hi - hi * f_lo) / (f_hi - f_lo)
        } else {
            0.5 * (lo + hi)
        };
        if !(lo < h && h < hi) {
            h = 0.5 * (lo + hi);
        }
        let Some(s) = at(h)? else {
            (lo, f_lo) = (h, f64::NEG_INFINITY);
            continue;
        };
        let f = s.tension_start - t;
        if f.abs() <= 1e-13 * t {
            return Ok(Some((h, s)));
        }
        if f > 0.0 {
            (hi, f_hi, best) = (h, f, (h, s));
            if side == 1 && f_lo.is_finite() {
                f_lo /= 2.0;
            }
            side = 1;
        } else {
            (lo, f_lo) = (h, f);
            if side == -1 {
                f_hi /= 2.0;
            }
            side = -1;
        }
    }
    Ok(Some(best))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two parabolic spans of load w: T = H + w (z_end - z_low), with
    /// z - z_low from the parabola (sag D = w S^2 / 8H, the low point's
    /// offset from the middle H h / (w S)); length S + 8D^2/3S + h^2/2S.
    #[test]
    fn two_parabolic_spans_have_equal_tension_at_the_sheave() {
        let w = 15.0;
        let spans = [(300.0, 0.0, 40.0), (250.0, 40.0, 10.0)];
        // The parabola from the back support, z = za + rise x/S - w x(S - x)/2H,
        // its vertex (the low point, inside the span or not) at
        // x_m = S/2 - H rise/(w S).
        let ends = |s: f64, za: f64, zb: f64, h: f64| {
            let z = |x: f64| za + (zb - za) * x / s - w * x * (s - x) / (2.0 * h);
            let low = z(s / 2.0 - h * (zb - za) / (w * s));
            (h + w * (za - low), h + w * (zb - low))
        };
        let length = |s: f64, rise: f64, h: f64| {
            let d = w * s * s / (8.0 * h);
            s + 8.0 * d * d / (3.0 * s) + rise * rise / (2.0 * s)
        };
        let total = length(300.0, 40.0, 20_000.0) + length(250.0, -30.0, 20_000.0);
        let hs = share(
            2,
            total,
            20_000.0,
            |i, h| -> Result<_, String> {
                let (s, za, zb) = spans[i];
                let (a, b) = ends(s, za, zb, h);
                Ok(Some(SheaveSpan {
                    unstressed: length(s, zb - za, h),
                    tension_start: a,
                    tension_end: b,
                }))
            },
            &Budget::new().max_iterations(200),
        )
        .unwrap();
        let (_, t1) = ends(300.0, 0.0, 40.0, hs[0]);
        let (t2, _) = ends(250.0, 40.0, 10.0, hs[1]);
        assert!((t1 - t2).abs() < 1e-6 * t1, "{t1} {t2}");
        let l = length(300.0, 40.0, hs[0]) + length(250.0, -30.0, hs[1]);
        assert!((l - total).abs() < 1e-9 * total);
        // The span below the sheave carries more H: B is high above its
        // low point in span 1 and less so in span 2.
        assert!(hs[1] > hs[0] + 100.0, "{hs:?}");
    }
}
