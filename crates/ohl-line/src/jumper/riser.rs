// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A jumper given by its tension (owner decision 2026-10-25): the riser
//! of a lyre pylon, dead-ended between two strain points of the same
//! structure and pulled to a tension, the jumpers at its top and bottom
//! tapping onto it. Any chord direction, vertical included.
//!
//! **Model.** A taut wire of \\( n \\) straight elastic bars (pieces of
//! about 1 m, 8 to 64) between its two wire points, its load lumped at the
//! nodes: half of each piece's weight, ice and wind on each of its ends,
//! its objects and the jumpers hanging on it shared between the two nodes
//! around them. The catenary element cannot take a chord along its load
//! (a vertical wire in still air); bars can, and a taut wire of them
//! bows in the wind and sags under its weight like the wire. Solved by
//! the Newton solver of `ohl-wirefe` (exit clauses there).
//!
//! **Its length.** The given tension is the pull at its higher end (its
//! `from` end when both are level), per subconductor, in still air at its
//! temperature, between its ends solved there (its design case, as for a
//! jumper given by its sag). The unstressed length giving it is found by a
//! bracketed secant (exit clause [`CUT_STEPS`]) and kept; in every
//! weather case the jumper keeps it (lengthened or shortened with the
//! temperature like the line's wires): its tension follows the weather.
//!
//! Simplifications (marked in the theory page): the load per metre of
//! unstressed wire (its stretch, \\( T/EA \\), about \\( 10^{-4} \\), left
//! out of the ice and wind); the polygon of bars for the curve (its error
//! reported, [`JumperShape::polyline_error`]: \\( q\\,\ell^2 / 8T \\) per
//! piece); no bending stiffness (a taut wire). A riser that goes slack (a
//! bar in compression) is refused for that weather case: given by its
//! tension, it must stay taut.

use super::*;

/// Steps of the search for the cut length (exit clause; a bracketed
/// secant converges in 5–15).
pub const CUT_STEPS: usize = 100;

/// Pieces of a riser of unstressed length `length`: about 1 m each, 8 to
/// 64.
fn pieces(length: f64) -> usize {
    ((length / 1.0).ceil() as usize).clamp(8, 64)
}

/// A riser solved between `a` and `b`: its node positions, the bars'
/// forces and the reactions at its two ends.
pub(super) struct Solved {
    pub points: Vec<Vector3<f64>>,
    pub forces: Vec<f64>,
    pub ends: [Vector3<f64>; 2],
}

/// The riser of unstressed length `l0` (at the case's temperature) between
/// `a` and `b` under `load` per unstressed metre and the point loads
/// `points` (share of its length, force; per subconductor). Err: it did
/// not solve, or a bar is slack (with the lowest force).
pub(super) fn solve_taut(
    a: Vector3<f64>,
    b: Vector3<f64>,
    l0: f64,
    ea: f64,
    load: Vector3<f64>,
    points: &[(f64, Vector3<f64>)],
) -> Result<Solved, String> {
    let n = pieces(l0);
    let piece = l0 / n as f64;
    let d = b - a;
    let chord = d.norm();
    if chord < 1e-3 {
        return Err("its ends are at the same point".into());
    }
    // Start: bowed along the load's part across the chord as a taut
    // string of that load would be (a parabola), its depth δ where the
    // tension the bow's stretch adds balances it:
    // δ (T₀ + EA 8δ²/3c²) = q c²/8, T₀ the straight wire's (Newton from
    // the straight wire stalled on a 30 m riser in a 10 N/m wind: the
    // string stiffens as it bows).
    let u = d / chord;
    let across = load - u * load.dot(&u);
    let q = across.norm();
    let bow = if q > 0.0 {
        let t0 = ea * (chord / l0 - 1.0);
        let f =
            |x: f64| x * (t0 + ea * 8.0 * x * x / (3.0 * chord * chord)) - q * chord * chord / 8.0;
        let (mut lo, mut hi) = (0.0, chord);
        // Exit clause: 200 halvings (to rounding).
        for _ in 0..200 {
            let mid = 0.5 * (lo + hi);
            if f(mid) < 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
            if hi - lo <= 1e-12 * chord {
                break;
            }
        }
        across / q * (0.5 * (lo + hi))
    } else {
        Vector3::zeros()
    };
    let mut model = Model::default();
    for k in 0..=n {
        let f = k as f64 / n as f64;
        // Relative to `a`: in project coordinates (~10^6 m) rounding
        // would swamp the tolerances.
        let at = d * f + bow * (4.0 * f * (1.0 - f));
        let mut node = if k == 0 || k == n {
            Node::fixed(at)
        } else {
            Node::free(at)
        };
        node.load = load * piece;
        model.nodes.push(node);
    }
    // The end nodes carry half a piece each.
    model.nodes[0].load /= 2.0;
    model.nodes[n].load /= 2.0;
    for (f, w) in points {
        let x = f.clamp(0.0, 1.0) * n as f64;
        let k = (x.floor() as usize).min(n - 1);
        let t = x - k as f64;
        model.nodes[k].load += w * (1.0 - t);
        model.nodes[k + 1].load += w * t;
    }
    for k in 0..n {
        model.trusses.push(TrussElement {
            i: k,
            j: k + 1,
            unstressed_length: piece,
            axial_stiffness: ea,
            tension_only: false,
            weight: 0.0,
        });
    }
    let solution = model
        .solve(&default_fe_budget())
        .map_err(|e| e.to_string())?;
    let lowest = solution
        .truss_forces
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    if lowest.is_nan() || lowest <= 0.0 {
        return Err(format!(
            "slack: its tension falls to {lowest:.0} N along it (given by its tension, it must \
             stay taut; a lower temperature or more tension)"
        ));
    }
    Ok(Solved {
        points: solution.positions.iter().map(|q| a + q).collect(),
        forces: solution.truss_forces.clone(),
        // The force the riser exerts on each end: the reaction reversed.
        ends: [-solution.reactions[0], -solution.reactions[n]],
    })
}

/// The wire's tension at its higher end (`from` when both are level):
/// its pull on that clamp.
fn top_tension(a: Vector3<f64>, b: Vector3<f64>, ends: &[Vector3<f64>; 2]) -> f64 {
    if b.z > a.z {
        ends[1].norm()
    } else {
        ends[0].norm()
    }
}

/// The riser's cut length for its tension between its ends `a`, `b` in
/// its design case (still air at its temperature), its objects on it
/// (the jumpers that tap onto it are hung after it is tensioned).
pub(super) fn design_length(
    line: &Line,
    jumper: &Jumper,
    cable: &Cable,
    tension: f64,
    a: Vector3<f64>,
    b: Vector3<f64>,
) -> Result<f64, String> {
    let still = WeatherCase::still(
        "riser design",
        ohl_core::units::Temperature::from_celsius(jumper.temperature),
    );
    let d = b - a;
    let along = line.direction_at([0.5 * (a.x + b.x), 0.5 * (a.y + b.y)]);
    let bundle = f64::from(line.jumper_bundle(jumper).count.max(1));
    // Placed along it by the chord (its length is within T/EA of it).
    let points: Vec<(f64, Vector3<f64>)> =
        object_loads(jumper, cable, &still, d.norm(), d.xy().norm(), along)
            .into_iter()
            .map(|(f, w)| (f, w / bundle))
            .collect();
    cut_length(
        a,
        b,
        tension,
        cable.weight().si(),
        cable.axial_stiffness().si(),
        &points,
    )
}

/// The unstressed length, at the jumper's temperature, giving `tension`
/// at the riser's higher end in still air between `a` and `b` (own weight
/// `w` per metre, axial stiffness `ea`, point loads `points`). Bracketed
/// secant on the length (a longer wire: less tension); a length too long
/// to stay taut counts as too little tension.
pub(super) fn cut_length(
    a: Vector3<f64>,
    b: Vector3<f64>,
    tension: f64,
    w: f64,
    ea: f64,
    points: &[(f64, Vector3<f64>)],
) -> Result<f64, String> {
    if tension.is_nan() || tension <= 0.0 {
        return Err(format!("its tension {tension} N must be above zero"));
    }
    let load = Vector3::new(0.0, 0.0, -w);
    let excess = |l: f64| -> Result<f64, String> {
        match solve_taut(a, b, l, ea, load, points) {
            Ok(s) => Ok(top_tension(a, b, &s.ends) - tension),
            Err(e) if e.starts_with("slack") => Ok(-tension),
            Err(e) => Err(e),
        }
    };
    let chord = (b - a).norm();
    // About the straight wire stretched by the tension.
    let guess = chord / (1.0 + tension / ea);
    let step = chord * tension / ea;
    let (mut lo, mut hi) = (guess - step, guess);
    let mut steps = 0;
    let mut count = || {
        steps += 1;
        if steps > CUT_STEPS {
            Err(format!(
                "its cut length for {tension:.0} N not found in {CUT_STEPS} steps"
            ))
        } else {
            Ok(())
        }
    };
    let mut f_lo = excess(lo)?;
    while f_lo < 0.0 {
        count()?;
        (hi, lo) = (lo, lo - 2.0 * (hi - lo));
        f_lo = excess(lo)?;
    }
    let mut f_hi = excess(hi)?;
    while f_hi > 0.0 {
        count()?;
        (lo, f_lo, hi) = (hi, f_hi, hi + 2.0 * (hi - lo));
        f_hi = excess(hi)?;
    }
    let mut side = 0;
    let mut l = 0.5 * (lo + hi);
    while hi - lo > 1e-12 * chord {
        count()?;
        l = (lo * f_hi - hi * f_lo) / (f_hi - f_lo);
        if !(lo < l && l < hi) {
            l = 0.5 * (lo + hi);
        }
        let f = excess(l)?;
        if f.abs() <= 1e-10 * tension {
            break;
        }
        if f > 0.0 {
            (lo, f_lo) = (l, f);
            if side == 1 {
                f_hi /= 2.0;
            }
            side = 1;
        } else {
            (hi, f_hi) = (l, f);
            if side == -1 {
                f_lo /= 2.0;
            }
            side = -1;
        }
    }
    Ok(l)
}

/// The riser `jumper` in `case` between its solved ends `a`, `b`, its cut
/// length `cut` (at its temperature), with the jumpers hanging on its
/// points (`hanging`, whole bundle, by share of its length).
#[allow(clippy::too_many_arguments)]
pub(super) fn shape(
    line: &Line,
    jumper: &Jumper,
    cable: &Cable,
    case: &WeatherCase,
    a: Vector3<f64>,
    b: Vector3<f64>,
    cut: f64,
    hanging: &dyn Fn(f64) -> Vec<(f64, Vector3<f64>)>,
) -> Result<JumperShape, String> {
    if !jumper.supports().is_empty() || jumper.support().is_some() {
        return Err(
            "given by its tension, with helping strings (post insulators along a riser are not \
             modelled yet)"
                .into(),
        );
    }
    let d = b - a;
    if d.xy().norm() < 0.1
        && jumper
            .points
            .iter()
            .any(|p| matches!(p.at, crate::JumperPlace::Distance(_)))
    {
        return Err(
            "a point placed by its distance along the line on a vertical riser: give its place \
             by its length or share"
                .into(),
        );
    }
    let ea = cable.axial_stiffness().si();
    let thermal = line
        .bimetallic
        .apply(cable)
        .free_thermal_strain(case.temperature.celsius() - jumper.temperature)
        .exp();
    let l0 = cut * thermal;
    let along = line.direction_at([0.5 * (a.x + b.x), 0.5 * (a.y + b.y)]);
    // Per unstressed metre: the own weight spread over the length (it
    // weighs the same hot or cold), ice and wind as on the jumper.
    let load = wire_load_in_line_wind(case, cable, d, along)
        + Vector3::new(0.0, 0.0, cable.weight().si() * (1.0 - 1.0 / thermal));
    let bundle = f64::from(line.jumper_bundle(jumper).count.max(1));
    let points: Vec<(f64, Vector3<f64>)> =
        object_loads(jumper, cable, case, l0, d.xy().norm(), along)
            .into_iter()
            .chain(hanging(l0))
            .map(|(f, w)| (f, w / bundle))
            .collect();
    let solved = solve_taut(a, b, l0, ea, load, &points)?;
    let n = solved.forces.len();
    let max_tension = solved
        .forces
        .iter()
        .copied()
        .chain(solved.ends.iter().map(|f| f.norm()))
        .fold(0.0, f64::max);
    // Each piece straight: the wire between its nodes bows by about
    // q l^2 / 8T across it.
    let piece = l0 / n as f64;
    let off = solved
        .points
        .windows(2)
        .zip(&solved.forces)
        .map(|(w, t)| {
            let u = (w[1] - w[0]).normalize();
            let across = (load - u * load.dot(&u)).norm();
            across * piece * piece / (8.0 * t)
        })
        .fold(0.0, f64::max);
    let low = *solved
        .points
        .iter()
        .min_by(|x, y| x.z.total_cmp(&y.z))
        .expect("points");
    let rts = cable.rated_tensile_strength.si().max(1e-9);
    Ok(JumperShape {
        jumper: jumper.id.clone(),
        sag: sag_below_chord(&solved.points, a, b),
        point_positions: point_positions(jumper, &solved.points, l0),
        points: solved.points.iter().map(|q| p(*q)).collect(),
        unstressed_length: l0,
        low_point: p(low),
        max_tension,
        end_forces: solved.ends.map(|f| p(f * bundle)),
        supports: Vec::new(),
        taut: true,
        strength_use: max_tension / rts,
        surface_radius: case.iced_diameter(cable.diameter).si() / 2.0,
        polyline_error: off,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vertical riser of 30 m under its own weight: straight, its
    /// tension falling by w L from the top; under a horizontal wind q, its
    /// middle deflects by about q L^2 / 8 T_mean (a taut string), T_mean
    /// the mean tension in the wind: the bow lengthens the wire by
    /// 8 δ² / 3L, which raises the tension by EA 8 δ² / 3L² (the length
    /// is kept), so δ solves δ = q L² / 8 (T₀ + EA 8 δ² / 3L²).
    #[test]
    fn a_vertical_riser_by_hand() {
        let (a, b) = (Vector3::new(0.0, 0.0, 40.0), Vector3::new(0.0, 0.0, 10.0));
        let (w, ea, t) = (15.0, 30e6, 5_000.0);
        let l = cut_length(a, b, t, w, ea, &[]).unwrap();
        let s = solve_taut(a, b, l, ea, Vector3::new(0.0, 0.0, -w), &[]).unwrap();
        let n = s.forces.len();
        // The pull on the top clamp is the given tension.
        assert!(
            (s.ends[0].norm() - t).abs() < 1e-6 * t,
            "{}",
            s.ends[0].norm()
        );
        // The bottom: T - w L (the whole weight hangs from the top).
        let bottom = s.ends[1].norm();
        assert!((bottom - (t - w * l)).abs() < 1e-6 * t, "{bottom}");
        assert!(
            s.points
                .iter()
                .all(|q| q.x.abs() < 1e-9 && q.y.abs() < 1e-9)
        );
        // Shorter than its chord by its mean stretch.
        let mean = t - w * l / 2.0;
        assert!((l - 30.0 / (1.0 + mean / ea)).abs() < 1e-6, "{l}");
        // A wind of 10 N/m across it.
        let q = 10.0;
        let s = solve_taut(a, b, l, ea, Vector3::new(q, 0.0, -w), &[]).unwrap();
        let mid = s.points[n / 2].x;
        let windy = s.forces.iter().sum::<f64>() / n as f64;
        let expected = q * 30.0 * 30.0 / (8.0 * windy);
        assert!((mid / expected - 1.0).abs() < 0.01, "{mid} vs {expected}");
        // By hand: the fixed point of the stretched bow.
        let mut delta = q * 900.0 / (8.0 * mean);
        for _ in 0..100 {
            delta = q * 900.0 / (8.0 * (mean + ea * 8.0 * delta * delta / (3.0 * 900.0)));
        }
        assert!((mid / delta - 1.0).abs() < 0.01, "{mid} vs {delta}");
        // Stiff, the bow is the string's under the given tension.
        assert!(delta < 0.75 * q * 900.0 / (8.0 * mean));
    }

    fn library() -> ohl_cables::Library {
        ohl_cables::Library::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap()
    }

    /// The example line with a riser on the structure of its first
    /// jumper's `from` end: two wire points fixed to the structure 12 m
    /// one above the other (sets 90, 91), the riser between them at 3 kN,
    /// its point K 3 m down from the top.
    fn with_riser() -> Line {
        let mut line = crate::tests::example();
        let end = line.jumpers[0].from.point().unwrap().clone();
        let st = line
            .structures
            .iter_mut()
            .find(|s| s.id == end.structure)
            .unwrap();
        let at = st.wire_at_set(end.set).unwrap().position;
        for (label, set, dz) in [("RT", 90, 6.0), ("RB", 91, -6.0)] {
            st.wires.push(crate::WirePoint {
                description: String::new(),
                label: label.into(),
                position: [at[0] + 1.5, at[1] + 1.5, at[2] + dz],
                set: Some(set),
                weight: 0.0,
            });
        }
        let mut riser = line.jumpers[0].clone();
        riser.id = "JR".into();
        riser.set_supports(Vec::new());
        riser.set_support(None);
        riser.points.clear();
        riser.sag = None;
        riser.length = None;
        riser.bending_stiffness = None;
        riser.tension = Some(3_000.0);
        riser.temperature = 15.0;
        riser.from = crate::JumperEnd::wire(end.structure.clone(), 90);
        riser.to = crate::JumperEnd::wire(end.structure.clone(), 91);
        riser.points.push(crate::JumperPoint {
            label: "K".into(),
            at: crate::JumperPlace::Length(3.0),
            description: String::new(),
            object: Some(crate::LineObject::clamp(20.0)),
        });
        line.jumpers.push(riser);
        line
    }

    /// The riser in the line: vertical, its pull at the top the given
    /// tension in still air at its temperature, its weight on the bottom;
    /// a jumper tapping onto it ends at its point and loads it; in the
    /// wind it bows across the line and its tension rises.
    #[test]
    fn a_riser_in_the_line_with_a_tap() {
        let lib = library();
        let calm = WeatherCase::still("15", ohl_core::units::Temperature::from_celsius(15.0));
        let mut line = with_riser();
        line.validate().unwrap();
        let solve = |line: &Line, case: &WeatherCase| {
            let shapes: Vec<_> = crate::solve::solve_line(line, &lib, case)
                .into_iter()
                .map(Result::unwrap)
                .collect();
            solve_jumpers(line, &lib, case, &shapes)
                .into_iter()
                .map(Result::unwrap)
                .collect::<Vec<_>>()
        };
        let bundle = f64::from(
            line.jumper_bundle(line.jumpers.last().unwrap())
                .count
                .max(1),
        );
        let shapes = solve(&line, &calm);
        let r = shapes.last().unwrap();
        let (top, bottom) = (r.points[0], *r.points.last().unwrap());
        assert!((top[2] - bottom[2] - 12.0).abs() < 1e-9);
        for q in &r.points {
            assert!((q[0] - top[0]).abs() < 1e-6 && (q[1] - top[1]).abs() < 1e-6);
        }
        let norm = |f: Point| (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt();
        assert!(
            (norm(r.end_forces[0]) / bundle - 3_000.0).abs() < 1e-3,
            "{:?}",
            r.end_forces
        );
        // Taut, down to the bottom by its weight and the clamp at K (cut
        // with it on).
        let jr = line.jumpers.iter().find(|j| j.id == "JR").unwrap();
        let cable = cut_cable(lib.get(&jr.cable).unwrap(), jr);
        let w = cable.weight().si() * r.unstressed_length + 20.0 / bundle;
        assert!(
            (norm(r.end_forces[1]) / bundle - (3_000.0 - w)).abs() < 0.5,
            "{} vs {}",
            norm(r.end_forces[1]) / bundle,
            3_000.0 - w
        );
        assert!(r.taut);
        // A jumper tapping onto K, to the host jumper's `from` end.
        let mut tap = line.jumpers[0].clone();
        tap.id = "JT".into();
        tap.points.clear();
        tap.set_supports(Vec::new());
        tap.set_support(None);
        tap.sag = None;
        tap.from = crate::JumperEnd::tap(crate::Tap::Jumper {
            jumper: "JR".into(),
            point: "K".into(),
        });
        tap.to = line.jumpers[0].from.clone();
        let a = approximate_end(&line, &tap.from, 0).unwrap();
        let b = approximate_end(&line, &tap.to, 0).unwrap();
        tap.length = Some(1.05 * (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt());
        line.jumpers.push(tap);
        line.validate().unwrap();
        let shapes = solve(&line, &calm);
        let (r2, t) = (&shapes[shapes.len() - 2], shapes.last().unwrap());
        let k = r2.point_positions[0].1;
        let gap = (0..3)
            .map(|i| (t.points[0][i] - k[i]).powi(2))
            .sum::<f64>()
            .sqrt();
        assert!(gap < 1e-9, "{gap}");
        // The tap's jumper hangs on K (its estimate, still air): the top
        // carries more.
        assert!(
            norm(r2.end_forces[0]) > norm(r.end_forces[0]) + 1.0,
            "{:?} {:?}",
            r2.end_forces,
            r.end_forces
        );
        // The wind across the line: it bows, its tension rises.
        let mut windy = calm.clone();
        windy.wind_pressure = ohl_core::units::Stress::from_pascals(500.0);
        let shapes = solve(&line, &windy);
        let r3 = &shapes[shapes.len() - 2];
        assert!(
            r3.max_tension > r2.max_tension,
            "{} {}",
            r3.max_tension,
            r2.max_tension
        );
        let mid = r3.points[r3.points.len() / 2];
        assert!(((mid[0] - top[0]).powi(2) + (mid[1] - top[1]).powi(2)).sqrt() > 0.01);
        // The file keeps it.
        let back = Line::from_json(&line.to_json()).unwrap();
        assert_eq!(back.jumpers, line.jumpers);
        assert!(line.to_json().contains("\"version\": 10,"));
    }
}
