// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Sagging outputs: what the stringing crew measures in the field, from
//! states the engine has already solved (post-processing only; the
//! derivations are in `docs/theory/sagtension.md`, "Sagging outputs").
//!
//! - [`stopwatch_time`]: the return time of \\( n \\) waves (the
//!   stopwatch method);
//! - the dynamometer reads the tension at the span's end
//!   ([`SpanSolution::tension_left`], [`SpanSolution::tension_right`]);
//! - [`angle_of_sight`]: the transit's angle to the tangent of the wire,
//!   and where the line of sight meets the far structure;
//! - [`level_sag`] / [`inclined_correction`]: the sag of an inclined span
//!   against the level-span sag of a chart;
//! - [`temperature_correction`]: the sag change per kelvin, to correct for
//!   the conductor temperature at sagging;
//! - [`clipping_offsets`]: the marks at the structures when a section
//!   sagged in the blocks at one condition is clipped in at another.
//!
//! The in-the-blocks model is the engine's (one horizontal tension through
//! the section, the strings plumb at stringing); these functions do not
//! change it.

use ohl_core::units::{Force, Length, STANDARD_GRAVITY};

use crate::catenary::{SagTensionError, SpanSolution};
use crate::section::SectionSolution;

/// Intervals of the composite Simpson rule of [`stopwatch_time`]; the
/// integrand \\( \sqrt{\cosh u} \\) is smooth, so the rule's error,
/// \\( O(h^4) \\), is below \\( 10^{-12} \\) of the time on any real span.
const SIMPSON_INTERVALS: usize = 1024;

/// Return time, s, of `waves` waves on a span: a pulse struck at one
/// support runs to the other and back `waves` times.
///
/// A transverse wave runs along a cable at \\( c = \sqrt{T/\mu} \\), with
/// \\( T \\) the local tension and \\( \mu = q/g \\) its mass per metre
/// (\\( q \\) the span's load per metre, the own weight spread over the
/// stretched cable, ice included if any). Along the catenary
/// \\( T = H\cosh u \\), \\( ds = \cosh u\\,dx \\), \\( u = (x - x_m)/C \\), so
///
/// \\[ t_n = 2n \int_0^L \frac{ds}{c} = 2n \sqrt{\frac{q}{gH}} \int_0^S
///     \sqrt{\cosh\frac{x - x_m}{C}}\\,dx \\]
///
/// (composite Simpson, 1024 intervals). For a taut level
/// span this is the field formula \\( t_n \approx 2n\sqrt{8D/g} \\), i.e.
/// \\( D \approx g t_n^2 / (32 n^2) \\).
pub fn stopwatch_time(span: &SpanSolution, waves: u32) -> f64 {
    let h = span.catenary.horizontal_tension.si();
    let q = span.catenary.weight.si();
    let c = h / q;
    let s = span.geometry.span.si();
    let xm = span.low_point_x().si();
    let f = |x: f64| ((x - xm) / c).cosh().sqrt();
    let n = SIMPSON_INTERVALS;
    let step = s / n as f64;
    let inner: f64 = (1..n)
        .map(|i| (if i % 2 == 1 { 4.0 } else { 2.0 }) * f(i as f64 * step))
        .sum();
    let integral = (f(0.0) + f(s) + inner) * step / 3.0;
    2.0 * f64::from(waves) * (q / (STANDARD_GRAVITY * h)).sqrt() * integral
}

/// The sag a chart gives from a measured return time: the field formula
/// \\( D = g t_n^2 / (32 n^2) \\) (taut level span, parabola).
pub fn stopwatch_sag(time: f64, waves: u32) -> Length {
    let n = f64::from(waves);
    Length::from_si(STANDARD_GRAVITY * time * time / (32.0 * n * n))
}

/// A transit's line of sight tangent to the wire ([`angle_of_sight`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sight {
    /// Angle of the line of sight above the horizontal, rad (negative:
    /// below).
    pub angle: f64,
    /// Horizontal position where it touches the wire (from the left
    /// support).
    pub tangent_x: Length,
    /// Elevation (relative to the left support) where it meets the
    /// vertical through `target_x`: where to set the target.
    pub target_elevation: Length,
}

/// The transit's angle of sight: from an instrument at horizontal position
/// `instrument_x` and elevation `instrument_z` (the span's frame: x from
/// the left support, elevations relative to it), the line tangent to the
/// wire beyond the instrument, and where it meets the vertical at
/// `target_x` (usually the far support).
///
/// The tangent at \\( x_t \\) passes through the instrument when
/// \\( f(x_t) = y(x_t) + y'(x_t)(x_i - x_t) - z_i = 0 \\). Since
/// \\( f'(x_t) = y''(x_t)(x_i - x_t) < 0 \\) for \\( x_t > x_i \\) (the
/// catenary is convex), there is one root beyond the instrument, found by
/// bisection between the instrument (or the left support) and the right
/// support. The instrument must be below the wire there, and the tangent
/// within the span; otherwise an error says so.
pub fn angle_of_sight(
    span: &SpanSolution,
    instrument_x: Length,
    instrument_z: Length,
    target_x: Length,
) -> Result<Sight, SagTensionError> {
    let (xi, zi) = (instrument_x.si(), instrument_z.si());
    let s = span.geometry.span.si();
    let y = |x: f64| span.elevation(Length::from_si(x)).si();
    let f = |x: f64| y(x) + span.slope(Length::from_si(x)) * (xi - x) - zi;
    let mut lo = xi.max(0.0);
    let mut hi = s;
    if !(xi < s && f(lo) > 0.0) {
        return Err(SagTensionError::InvalidInput(
            "the transit must be below the wire, before the far support".into(),
        ));
    }
    if f(hi) > 0.0 {
        return Err(SagTensionError::InvalidInput(
            "the line of sight touches the wire beyond the far support: set the transit higher"
                .into(),
        ));
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if f(mid) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo <= 1e-12 * s {
            break;
        }
    }
    let xt = 0.5 * (lo + hi);
    let slope = span.slope(Length::from_si(xt));
    Ok(Sight {
        angle: slope.atan(),
        tangent_x: Length::from_si(xt),
        target_elevation: Length::from_si(zi + slope * (target_x.si() - xi)),
    })
}

/// The sag of a level span of the same horizontal length, tension and load
/// per metre: \\( C(\cosh(S/2C) - 1) \\), what a level-span sag chart gives.
pub fn level_sag(span: &SpanSolution) -> Length {
    let c = span.catenary.parameter().si();
    Length::from_si(c * ((span.geometry.span.si() / (2.0 * c)).cosh() - 1.0))
}

/// The sag correction of an inclined span: its sag (measured vertically
/// from the chord, where the wire is parallel to it) minus the level-span
/// sag ([`level_sag`]). About \\( D_{level}(1/\cos\theta - 1) \\), with
/// \\( \theta \\) the chord's angle (the load per metre of wire).
pub fn inclined_correction(span: &SpanSolution) -> Length {
    span.sag() - level_sag(span)
}

/// Sag change per kelvin of each span, m/K: central difference of the
/// section solved at \\( t - \Delta t \\) and \\( t + \Delta t \\)
/// (`colder`, `warmer`, the difference `2 dt` kelvin).
pub fn temperature_correction(
    colder: &SectionSolution,
    warmer: &SectionSolution,
    dt: f64,
) -> Vec<f64> {
    colder
        .spans
        .iter()
        .zip(&warmer.spans)
        .map(|(a, b)| (b.sag() - a.sag()).si() / (2.0 * dt))
        .collect()
}

/// Clipping offsets, m, at the structures of a section (one per span
/// boundary, the far dead-end included: 0 by construction).
///
/// A section sagged in the blocks at one condition keeps, per span, the
/// unstressed lengths `sagged`; clipped in at another condition, the cable
/// has rolled through the sheaves to `clipping` (the engine's in-the-blocks
/// model: one horizontal tension). At structure \\( i \\) (after span
/// \\( i \\)) the wire point to clamp lies
///
/// \\[ \delta_i = \sum_{j \le i} (L_{0,j}^{sag} - L_{0,j}^{clip}) \\]
///
/// along the wire from the point at the sheave, positive into the span
/// ahead. Both lists add up to the section's unstressed length, so the
/// last offset is zero. In metres of unstressed wire (the stretched wire's
/// is \\( 1 + \varepsilon \\), about \\( 10^{-3} \\), longer).
pub fn clipping_offsets(sagged: &[f64], clipping: &[f64]) -> Vec<f64> {
    sagged
        .iter()
        .zip(clipping)
        .scan(0.0, |sum, (a, b)| {
            *sum += a - b;
            Some(*sum)
        })
        .collect()
}

/// The tension a dynamometer at the given end of a span reads (the cable's
/// tension there, along it).
pub fn dynamometer(span: &SpanSolution, at_left: bool) -> Force {
    if at_left {
        span.tension_left()
    } else {
        span.tension_right()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catenary::{Catenary, SpanGeometry};
    use ohl_core::units::ForcePerLength;

    fn span(s: f64, h: f64, rise: f64) -> SpanSolution {
        Catenary::new(
            Force::from_si(h),
            ForcePerLength::from_newtons_per_meter(15.69064),
        )
        .unwrap()
        .solve(SpanGeometry::inclined(
            Length::from_meters(s),
            Length::from_meters(rise),
        ))
        .unwrap()
    }

    #[test]
    fn stopwatch_matches_the_field_formula_on_a_taut_span() {
        let sp = span(300.0, 25_000.0, 0.0);
        let t3 = stopwatch_time(&sp, 3);
        // Field formula: D = g t^2 / (32 n^2), within the parabola's 0.3 %.
        let d = stopwatch_sag(t3, 3).meters();
        assert!((d / sp.sag().meters() - 1.0).abs() < 3e-3, "{d}");
        // Three waves take three times one.
        assert!((t3 - 3.0 * stopwatch_time(&sp, 1)).abs() < 1e-12);
    }

    #[test]
    fn transit_satisfies_the_parabola_rule() {
        // Level span, instrument a below the left support, target b below
        // the right: sqrt(a) + sqrt(b) = 2 sqrt(D) (parabola).
        let sp = span(300.0, 25_000.0, 0.0);
        let a = 3.0;
        let sight = angle_of_sight(
            &sp,
            Length::ZERO,
            Length::from_meters(-a),
            Length::from_meters(300.0),
        )
        .unwrap();
        let b = -sight.target_elevation.meters();
        let d = sp.sag().meters();
        assert!((a.sqrt() + b.sqrt() - 2.0 * d.sqrt()).abs() < 2e-3, "{b}");
        // The line touches the wire: elevation equal at the tangent point.
        let xt = sight.tangent_x;
        let line = -a + sight.angle.tan() * xt.meters();
        assert!((line - sp.elevation(xt).meters()).abs() < 1e-9);
        // Above the wire: refused.
        assert!(
            angle_of_sight(
                &sp,
                Length::from_meters(150.0),
                Length::ZERO,
                Length::from_meters(300.0)
            )
            .is_err()
        );
    }

    #[test]
    fn offsets_add_up_to_zero_at_the_far_dead_end() {
        let o = clipping_offsets(&[200.1, 350.2, 250.15], &[200.102, 350.199, 250.149]);
        assert!((o[0] + 0.002).abs() < 1e-12);
        assert!(o[2].abs() < 1e-12);
    }

    #[test]
    fn inclined_correction_is_about_the_secant() {
        let sp = span(300.0, 25_000.0, 50.0);
        let cos = 300.0 / 300f64.hypot(50.0);
        let expected = level_sag(&sp).meters() * (1.0 / cos - 1.0);
        assert!((inclined_correction(&sp).meters() - expected).abs() < 2e-3);
    }
}
