// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Jumpers whose shape jumps between the swing band's solved pressures
//! (2026-11-17, the owner's decision): a stiff jumper held at points can go
//! over to a near shape of its own inside a step of the band (on a real
//! line one swapped between two shapes 0.2 m apart at 100–135 Pa, 0.62 m
//! outside its band of still air, 150 and 300 Pa). Where a jumper's path
//! along the wind ([`ohl_line::jumper::JumperShape::jumps`]) jumped between
//! two solved pressures of a side, that jumper alone is solved every
//! [`SWAP_STEP`] over that interval only — with the sections at its
//! structures ([`jumpers_part`]) — and its band holds those shapes too
//! (rising and falling).

use ohl_core::budget::Budget;
use ohl_line::Line;
use ohl_line::jumper::JumperShape;
use ohl_weather::{WeatherCase, WindSide};

/// The pressure step a jumper whose shape jumps is solved at over the
/// interval it jumped in, Pa: the band levels' rounding and the finest wind
/// steps measured.
pub const SWAP_STEP: f64 = crate::positions::BAND_LEVEL_ROUNDING;

/// At most this many pressures a side are solved for the jumpers of one
/// case (exit clause; a 1500 Pa case has 150 steps of 10 Pa a side).
pub const MAX_SWAP_PRESSURES: usize = 300;

/// A jump of a jumper's shape on its way along the wind of one side.
#[derive(Debug, Clone, PartialEq)]
pub struct Swap {
    /// The side the wind blows from.
    pub side: WindSide,
    /// The jumper.
    pub jumper: String,
    /// Where it jumped, Pa (the case's pressure).
    pub at: f64,
}

/// The pressures to solve for the jumps `swaps` between the pressures
/// `solved` of each side (still air, the levels, the steps, the full
/// pressure): every [`SWAP_STEP`] strictly inside each interval
/// (p, q] of a side holding a jump, with the jumpers that jumped there —
/// (side, pressure, jumpers), in order.
pub fn swap_pressures(
    solved: &[(WindSide, f64)],
    swaps: &[Swap],
) -> Vec<(WindSide, f64, Vec<String>)> {
    let mut out: Vec<(WindSide, f64, Vec<String>)> = Vec::new();
    for sw in swaps {
        let mut at: Vec<f64> = solved
            .iter()
            .filter(|(s, _)| *s == sw.side)
            .map(|(_, p)| *p)
            .collect();
        at.sort_by(f64::total_cmp);
        at.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        let Some(k) = at
            .windows(2)
            .position(|w| w[0] < sw.at && sw.at <= w[1] + 1e-9)
        else {
            continue;
        };
        let (from, to) = (at[k], at[k + 1]);
        let mut y = from + SWAP_STEP;
        while y < to - 1e-9 {
            match out
                .iter_mut()
                .find(|(s, p, _)| *s == sw.side && (*p - y).abs() < 1e-9)
            {
                Some(e) => {
                    if !e.2.contains(&sw.jumper) {
                        e.2.push(sw.jumper.clone());
                    }
                }
                None => out.push((sw.side, y, vec![sw.jumper.clone()])),
            }
            y += SWAP_STEP;
        }
    }
    out.sort_by(|a, b| {
        (a.0 == WindSide::Left)
            .cmp(&(b.0 == WindSide::Left))
            .then(a.1.total_cmp(&b.1))
    });
    let mut kept: Vec<(WindSide, f64, Vec<String>)> = Vec::new();
    for side in [WindSide::Right, WindSide::Left] {
        kept.extend(
            out.iter()
                .filter(|x| x.0 == side)
                .take(MAX_SWAP_PRESSURES)
                .cloned(),
        );
    }
    kept
}

/// One solved pressure of a side for [`cross_swaps`]: the pressure, the
/// case as solved there, its sections, its jumpers as the wind rose (the
/// first of each id).
pub struct Solved<'a> {
    /// Pa.
    pub pressure: f64,
    /// The case as solved.
    pub case: &'a WeatherCase,
    /// Its sections.
    pub shapes: &'a [ohl_line::solve::SectionShape],
    /// Its jumpers (the first shape of each id: as the wind rose).
    pub jumpers: &'a [JumperShape],
}

/// The jumpers whose shapes at two neighbouring solved pressures of a side
/// do not lead to each other (2026-11-17): each jumper solved at the upper
/// pressure from its shape at the lower one, and at the lower from its
/// shape at the upper ([`ohl_line::jumper::solve_jumper_from`]); landing
/// more than [`ohl_line::jumper::RISE_TOLERANCE`] from the shape solved
/// there, it went over to another shape between them — with its ends
/// moving with the wind, which each solve's own path does not (its ends
/// those of the pressure solved: on a real line one jumper's shape at 120
/// and 130 Pa was the other one of its 100, 110 and 140 Pa, each path
/// smooth). A [`Swap`] at the upper pressure for each (one whose solve
/// fails too). Jumpers on taps on other jumpers are left out, and so are
/// shapes of two models (a stiff jumper gone taut: solved as a cable).
///
/// Expected cost: two rod solves from a near shape per stiff jumper and
/// pair (~1 ms each in release). Exit clause: none of its own (each solve's).
pub fn cross_swaps(
    line: &Line,
    library: &ohl_cables::Library,
    side: WindSide,
    lower: &Solved,
    upper: &Solved,
) -> Vec<Swap> {
    // (Shapes of other models — a stiff jumper gone taut, solved as a
    // cable — have other points: not compared.)
    let far = |a: &[[f64; 3]], b: &[[f64; 3]]| {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .any(|(x, y)| crate::geometry::distance(*x, *y) > ohl_line::jumper::RISE_TOLERANCE)
    };
    let first = |list: &[JumperShape], id: &str| list.iter().find(|j| j.jumper == id).cloned();
    let mut out = Vec::new();
    for def in &line.jumpers {
        // (On a tap on another jumper: that tap not known alone.)
        if def
            .taps()
            .any(|t| matches!(t, ohl_line::Tap::Jumper { .. }))
        {
            continue;
        }
        let (Some(lo), Some(up)) = (first(lower.jumpers, &def.id), first(upper.jumpers, &def.id))
        else {
            continue;
        };
        if lo.points.len() != up.points.len() {
            continue;
        }
        let leads = |at: &Solved, from: &JumperShape, own: &JumperShape| {
            ohl_line::jumper::solve_jumper_from(
                line,
                library,
                at.case,
                at.shapes,
                def,
                from.points.clone(),
            )
            .is_ok_and(|s| !far(&s.points, &own.points))
        };
        if !leads(upper, &lo, &up) || !leads(lower, &up, &lo) {
            out.push(Swap {
                side,
                jumper: def.id.clone(),
                at: upper.pressure,
            });
        }
    }
    out
}

/// The part of `line` the jumpers `ids` need solved: those jumpers and the
/// jumpers they hang on (taps on jumpers, all the way), and the sections
/// with a point at any structure they touch (their ends, their helping
/// strings) or tapped, with the sections those share an assembly or a
/// spacer with. At rigid structures each section solves as in the whole
/// line; with flexible structures (calculation levels 3 and 4) those
/// structures move less without the wires left out (the shapes added here
/// widen the band only: nothing checked is taken away).
pub fn jumpers_part(line: &Line, ids: &[String]) -> Line {
    let mut want: Vec<String> = ids.to_vec();
    let mut k = 0;
    while k < want.len() {
        if let Some(j) = line.jumpers.iter().find(|j| j.id == want[k]) {
            for t in j.taps() {
                if let ohl_line::Tap::Jumper { jumper, .. } = t
                    && !want.contains(jumper)
                {
                    want.push(jumper.clone());
                }
            }
        }
        k += 1;
    }
    let jumpers: Vec<&ohl_line::Jumper> = line
        .jumpers
        .iter()
        .filter(|j| want.contains(&j.id))
        .collect();
    let mut structures: Vec<String> = Vec::new();
    let mut tapped: Vec<&str> = Vec::new();
    for j in &jumpers {
        structures.extend(j.points().map(|p| p.structure.clone()));
        for c in j.supports() {
            structures.push(c.structure_id(j).to_owned());
        }
        for t in j.taps() {
            if let ohl_line::Tap::Span(r) = t {
                tapped.push(r.section.as_str());
            }
        }
    }
    let mut keep: Vec<bool> = line
        .sections
        .iter()
        .map(|s| {
            tapped.contains(&s.id.as_str())
                || s.points.iter().any(|p| structures.contains(&p.structure))
        })
        .collect();
    for group in ohl_line::solve::coupled_groups(line)
        .into_iter()
        .chain(ohl_line::solve::spacer_groups(line))
    {
        if group.iter().any(|&k| keep[k]) {
            for k in group {
                keep[k] = true;
            }
        }
    }
    let mut part = line.clone();
    let mut k = 0;
    part.sections.retain(|_| {
        k += 1;
        keep[k - 1]
    });
    part.jumpers.retain(|j| want.contains(&j.id));
    part
}

/// The jumpers solved at one pressure of a side ([`solve_swaps`]).
#[derive(Debug, Clone)]
pub struct SwapSolve {
    /// The side the wind blows from.
    pub side: WindSide,
    /// The pressure, Pa.
    pub pressure: f64,
    /// The jumpers asked for: as the wind rises, then those with another
    /// shape as it falls from the case's own pressure
    /// ([`JumperShape::fallen`]).
    pub jumpers: Vec<JumperShape>,
    /// What could not be solved.
    pub failed: Vec<String>,
}

/// Solves the jumpers of `wanted` ([`swap_pressures`]) at their pressures:
/// for each, the case `at(side, pressure)` on [`jumpers_part`] of the line,
/// the jumpers also as the wind falls from `full` (the case's own pressure);
/// in parallel.
///
/// Expected cost: per pressure one solve of the sections at the jumpers'
/// structures (a few ms) and the jumpers' paths along the wind (~1–10 ms
/// each). Exit clause: `budget`'s time limit, checked between solves.
pub fn solve_swaps(
    line: &Line,
    library: &ohl_cables::Library,
    wanted: &[(WindSide, f64, Vec<String>)],
    full: f64,
    at: impl Fn(WindSide, f64) -> WeatherCase + Sync,
    budget: &Budget,
) -> Result<Vec<SwapSolve>, String> {
    let fe = ohl_wirefe::default_fe_budget();
    crate::positions::parallel(wanted.len(), budget, |k| {
        let (side, pressure, ids) = &wanted[k];
        let part = jumpers_part(line, ids);
        let case = at(*side, *pressure);
        let mut failed = Vec::new();
        let shapes: Vec<_> = ohl_line::solve::solve_line_memo(&part, library, &case, &fe)
            .into_iter()
            .zip(&part.sections)
            .filter_map(|(r, s)| {
                r.map_err(|e| failed.push(format!("section {} not solved: {e}", s.id)))
                    .ok()
            })
            .collect();
        let path = ohl_line::jumper::WindPath {
            falling_from: Some(full),
            still_shapes: false,
        };
        let mut jumpers = Vec::new();
        let mut fallen = Vec::new();
        for (j, r) in part.jumpers.iter().zip(ohl_line::jumper::solve_jumpers_on(
            &part, library, &case, &shapes, path,
        )) {
            if !ids.contains(&j.id) {
                continue;
            }
            match r {
                Ok(s) => {
                    fallen.extend(s.fallen());
                    jumpers.push(s);
                }
                Err(e) => failed.push(format!("jumper {} not solved: {e}", j.id)),
            }
        }
        jumpers.extend(fallen);
        Ok(SwapSolve {
            side: *side,
            pressure: *pressure,
            jumpers,
            failed,
        })
    })
}

/// What a case's results say of its jumpers solved between the band's
/// pressures ([`swap_pressures`]): which, from where, over what. None
/// without any.
pub fn swaps_note(case: &str, wanted: &[(WindSide, f64, Vec<String>)]) -> Option<String> {
    if wanted.is_empty() {
        return None;
    }
    // Per jumper and side: the pressures solved.
    let mut by: Vec<(String, WindSide, f64, f64)> = Vec::new();
    for (side, p, ids) in wanted {
        for id in ids {
            match by.iter_mut().find(|x| &x.0 == id && x.1 == *side) {
                Some(x) => {
                    x.2 = x.2.min(*p);
                    x.3 = x.3.max(*p);
                }
                None => by.push((id.clone(), *side, *p, *p)),
            }
        }
    }
    by.sort_by(|a, b| a.0.cmp(&b.0));
    let list: Vec<String> = by
        .iter()
        .map(|(j, side, a, b)| format!("{j} from the {}, {a:.0}–{b:.0} Pa", side.as_str()))
        .collect();
    let ids: std::collections::BTreeSet<&str> = by.iter().map(|x| x.0.as_str()).collect();
    Some(format!(
        "case \"{case}\": {} jumper(s) whose shape jumps between the swing band's solved \
         pressures, solved every {SWAP_STEP:.0} Pa there and held by their bands ({}; {} more \
         solves of the sections at their structures)",
        ids.len(),
        list.join(", "),
        wanted.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each jump between two solved pressures of its side gives the
    /// pressures every 10 Pa strictly inside that interval, the jumpers
    /// merged by pressure; a jump at a solved pressure takes the interval
    /// below it; the other side's pressures are not used.
    #[test]
    fn a_jump_is_solved_every_ten_pascal_over_its_interval() {
        let solved = [
            (WindSide::Right, 0.0),
            (WindSide::Right, 150.0),
            (WindSide::Right, 300.0),
            (WindSide::Left, 0.0),
            (WindSide::Left, 100.0),
            (WindSide::Left, 300.0),
        ];
        let swap = |side, jumper: &str, at| Swap {
            side,
            jumper: jumper.into(),
            at,
        };
        let w = swap_pressures(
            &solved,
            &[
                swap(WindSide::Right, "a", 130.0),
                swap(WindSide::Right, "b", 145.0),
                swap(WindSide::Right, "c", 150.0),
                swap(WindSide::Left, "a", 290.0),
            ],
        );
        let right: Vec<f64> = w
            .iter()
            .filter(|x| x.0 == WindSide::Right)
            .map(|x| x.1)
            .collect();
        assert_eq!(
            right,
            (1..15).map(|k| f64::from(k) * 10.0).collect::<Vec<_>>()
        );
        assert!(
            w.iter()
                .filter(|x| x.0 == WindSide::Right)
                .all(|x| x.2 == ["a", "b", "c"])
        );
        let left: Vec<f64> = w
            .iter()
            .filter(|x| x.0 == WindSide::Left)
            .map(|x| x.1)
            .collect();
        assert_eq!(left.len(), 19);
        assert_eq!(left[0], 110.0);
        assert_eq!(*left.last().unwrap(), 290.0);
        let note = swaps_note("storm", &w).unwrap();
        assert!(note.contains("a from the right, 10–140 Pa"), "{note}");
        assert!(note.contains("a from the left, 110–290 Pa"), "{note}");
    }
}
