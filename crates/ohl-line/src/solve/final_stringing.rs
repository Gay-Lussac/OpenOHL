// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A stringing value given in a final condition (roadmap O22 item 3): the
//! tension, the sag or the catenary constant holds **after creep** or
//! **after load** ([`crate::StrungAt`]), and the initial stringing — what
//! every solve starts from — is worked back from it.
//!
//! The unknown is the initial horizontal tension \\( H_0 \\) at the
//! stringing temperature. \\( F(H_0) \\) is the mean horizontal tension of
//! the section strung initially at \\( H_0 \\) and solved in still air at
//! the stringing temperature in the condition asked, by the section's own
//! method (finite elements or ruling span), the wire alone (no jumpers, no
//! spacers: as the stringing reference). \\( F \\) rises with \\( H_0 \\)
//! and \\( F(H_0) \le H_0 \\) (a permanent stretch only lengthens the
//! wire), so the root of \\( F(H_0) = H^* \\) is bracketed from
//! \\( H^* \\) upwards and found by the Illinois secant.
//!
//! On the safe side: the initial tension found is **higher** than the
//! value given, and the initial state keeps giving the high tensions and
//! the structure loads; the final sags are the ones asked.

use super::*;
use crate::StrungAt;

/// Section solves at most when working a stringing back from a final
/// condition (exit clause; it settles in 6–12).
pub const WORK_BACK_SOLVES: usize = 60;

/// What a section in the blocks is strung to, and the initial stringing
/// that gives it ([`worked_back`]).
#[derive(Debug, Clone, PartialEq)]
pub struct WorkedBack {
    /// The condition the value given holds in.
    pub condition: StrungAt,
    /// Stringing temperature, °C.
    pub temperature: f64,
    /// The horizontal tension asked for in that condition, N per wire (the
    /// tension given, the catenary constant times the weight as strung, or
    /// the tension of the sag given).
    pub target: f64,
    /// The initial horizontal tension that gives it, N per wire (the
    /// target itself in the initial condition).
    pub initial: f64,
    /// Section solves it took (0 in the initial condition).
    pub solves: usize,
    /// What the reader must know besides (a linear cable strung "after
    /// load": no permanent stretch, the same as initial).
    pub note: Option<String>,
}

impl WorkedBack {
    /// One line for a report: "sagged to 22.00 kN after creep at 15 °C:
    /// 23.31 kN initial" (the loads on the structures come from the
    /// initial tension, never to be read off the final value).
    pub fn text(&self) -> String {
        let kn = |n: f64| n / 1000.0;
        if self.condition.is_initial() {
            return format!(
                "sagged to {:.2} kN (initial) at {} °C",
                kn(self.target),
                self.temperature
            );
        }
        let mut text = format!(
            "sagged to {:.2} kN {} at {} °C: {:.2} kN initial",
            kn(self.target),
            self.condition.label(),
            self.temperature,
            kn(self.initial)
        );
        if let Some(n) = &self.note {
            text.push_str(&format!(" ({n})"));
        }
        text
    }
}

/// The horizontal tension (N per wire) a section in the blocks asks for in
/// its stringing condition, between the plumb `clamps`: the tension given,
/// the catenary constant times the bare weight per metre of `cable` (the
/// cable as strung, [`strung_cable`]), or the tension of the sag given.
pub(super) fn asked_tension(
    section: &Section,
    clamps: &[Vector3<f64>],
    cable: &Cable,
) -> Result<f64, LineError> {
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    let w = cable.weight().si();
    match &section.stringing {
        Stringing::Tension {
            horizontal_tension, ..
        } => Ok(*horizontal_tension),
        Stringing::CatenaryConstant { constant, .. } => Ok(constant * w),
        Stringing::Sag { sag, span, .. } => tension_for_sag(clamps, *span, *sag, w)
            .map_err(|m| invalid(format!("sag {sag} m: {m}"))),
        Stringing::UnstressedLengths { .. } => {
            Err(invalid("clipped in: no stringing tension".into()))
        }
    }
}

/// `section` with its stringing value taken as an initial one (the same
/// kind and value).
fn taken_as_initial(section: &Section) -> Section {
    let mut s = section.clone();
    match &mut s.stringing {
        Stringing::Tension { condition, .. }
        | Stringing::Sag { condition, .. }
        | Stringing::CatenaryConstant { condition, .. } => *condition = StrungAt::Initial,
        Stringing::UnstressedLengths { .. } => {}
    }
    s
}

/// The mean horizontal tension of a solved section's spans, N per wire.
fn mean_horizontal(shape: &SectionShape) -> f64 {
    shape
        .spans
        .iter()
        .map(|s| s.horizontal_tension)
        .sum::<f64>()
        / shape.spans.len().max(1) as f64
}

/// Works the stringing of `section` (in the blocks, its value in a final
/// condition) back to its initial horizontal tension: see the module's
/// text. `holders` are the section's, `cable` the cable as strung.
///
/// Exit clauses: [`WORK_BACK_SOLVES`] section solves, or the budget's time
/// limit and cancellation; the bracket stops at the wire's rated tensile
/// strength (when it gives one).
///
/// Expected cost: 6–12 solves of the section in still air, each with its
/// own stringing reference and final state (10–100 ms for 10 spans): 0.1–1
/// s per section, once (kept in the memo of the section's parts).
pub(super) fn work_back(
    line: &Line,
    section: &Section,
    holders: &[Holder],
    cable: &Cable,
    budget: &Budget,
) -> Result<WorkedBack, LineError> {
    use crate::condition::Calculation;
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    let condition = section.stringing.condition();
    let temperature = section.stringing.temperature();
    // The target: for a sag, between the clamps an initial stringing of
    // the same value hangs plumb from.
    let target = match &section.stringing {
        Stringing::Sag { .. } => {
            let initial = taken_as_initial(section);
            let (layout, _) = estimate_strain_weights(&initial, holders.to_vec(), cable, budget)?;
            let clamps: Vec<Vector3<f64>> = layout.iter().map(plumb).collect();
            asked_tension(section, &clamps, cable)?
        }
        _ => asked_tension(section, &[], cable)?,
    };
    let still = WeatherCase::still(
        "stringing",
        ohl_core::units::Temperature::from_celsius(temperature),
    );
    // The wire alone, as the stringing reference: no jumpers, no spacers.
    let alone = Coupling {
        jumpers: Some(JumperLoadMap::new()),
        ..Coupling::default()
    };
    let mut trial = section.clone();
    // Nothing hangs on it yet either (markers, spacer clamps, spread
    // loads): the stringing reference is of the bare wire.
    trial.span_points.clear();
    trial.spreads.clear();
    trial.calculation = Calculation::of(condition.as_condition(), section.calculation.method());
    let run = budget.start();
    let solves = std::cell::Cell::new(0usize);
    let after = condition.label();
    let mut final_tension = |h0: f64| -> Result<f64, LineError> {
        run.check().map_err(|e| {
            invalid(format!(
                "stringing {after}: {e} ({} solves done)",
                solves.get()
            ))
        })?;
        solves.set(solves.get() + 1);
        trial.stringing = Stringing::tension(h0, temperature);
        let shape =
            solve_cases(line, &trial, cable, Cases::one(&still), budget, &alone).map_err(|e| {
                invalid(format!(
                    "stringing {after}, tried at {:.2} kN initial: {e}",
                    h0 / 1000.0
                ))
            })?;
        Ok(mean_horizontal(&shape))
    };
    let close = |f: f64| (f - target).abs() <= 1e-9 * target;
    let done = |initial: f64, solves: usize, note: Option<String>| WorkedBack {
        condition,
        temperature,
        target,
        initial,
        solves,
        note,
    };
    // The bracket: no stretch at all is the low end, F(H) ≤ H.
    let (mut lo, mut f_lo) = (target, final_tension(target)? - target);
    if f_lo >= -1e-9 * target {
        let note = "no permanent stretch in this condition: the same as an initial value";
        return Ok(done(target, solves.get(), Some(note.to_owned())));
    }
    let rts = cable.rated_tensile_strength.si();
    let ceiling = if rts.is_finite() && rts > 0.0 {
        rts
    } else {
        f64::INFINITY
    };
    if target >= ceiling {
        return Err(invalid(format!(
            "the tension {after} of {:.2} kN is not below the wire's rated tensile strength \
             ({:.2} kN)",
            target / 1000.0,
            rts / 1000.0
        )));
    }
    let (mut hi, mut f_hi);
    loop {
        hi = (lo * 1.25).min(ceiling);
        f_hi = final_tension(hi)? - target;
        if f_hi >= 0.0 {
            break;
        }
        if hi >= ceiling {
            return Err(invalid(format!(
                "the tension {after} of {:.2} kN cannot be reached below the wire's rated \
                 tensile strength: strung initially at the RTS ({:.2} kN) the section has \
                 {:.2} kN {after}",
                target / 1000.0,
                rts / 1000.0,
                (f_hi + target) / 1000.0
            )));
        }
        (lo, f_lo) = (hi, f_hi);
        if solves.get() >= WORK_BACK_SOLVES {
            return Err(not_settled(section, after, lo, hi, f_lo, solves.get()));
        }
    }
    // Illinois secant inside the bracket.
    let mut side = 0;
    while solves.get() < WORK_BACK_SOLVES {
        if close(f_hi + target) {
            return Ok(done(hi, solves.get(), None));
        }
        if close(f_lo + target) {
            return Ok(done(lo, solves.get(), None));
        }
        // The bracket at the numbers' resolution: the solves' own
        // tolerance is all that is left of the mismatch.
        if hi - lo <= 4.0 * f64::EPSILON * hi {
            if f_hi.abs().min(f_lo.abs()) <= 1e-6 * target {
                let initial = if f_hi.abs() < f_lo.abs() { hi } else { lo };
                return Ok(done(initial, solves.get(), None));
            }
            break;
        }
        let x = (lo * f_hi - hi * f_lo) / (f_hi - f_lo);
        let x = if x > lo && x < hi { x } else { 0.5 * (lo + hi) };
        let f = final_tension(x)? - target;
        if f >= 0.0 {
            (hi, f_hi) = (x, f);
            if side == 1 {
                f_lo *= 0.5;
            }
            side = 1;
        } else {
            (lo, f_lo) = (x, f);
            if side == -1 {
                f_hi *= 0.5;
            }
            side = -1;
        }
    }
    Err(not_settled(
        section,
        after,
        lo,
        hi,
        f_lo.abs().min(f_hi.abs()),
        solves.get(),
    ))
}

/// The error of a working back that did not settle.
fn not_settled(
    section: &Section,
    after: &str,
    lo: f64,
    hi: f64,
    mismatch: f64,
    solves: usize,
) -> LineError {
    LineError::Invalid(format!(
        "section {}: stringing {after} did not settle (bracket [{lo:.3}, {hi:.3}] N, mismatch \
         {:.3e} N after {solves} solves)",
        section.id,
        mismatch.abs()
    ))
}

/// What `section` is strung to and the initial horizontal tension that
/// gives it (none: clipped in — its lengths are the initial cut lengths).
/// For a value in a final condition the initial tension is worked back
/// (the module's text) and is the higher one: structure loads follow from
/// it, so a report must say it beside the value given
/// ([`WorkedBack::text`]).
///
/// Expected cost: nothing after a solve of the section (from the memo);
/// else the section's stringing reference, and 6–12 section solves for a
/// final condition (0.1–1 s for 10 spans).
pub fn worked_back(
    line: &Line,
    section: &Section,
    cable: &Cable,
    budget: &Budget,
) -> Result<Option<WorkedBack>, LineError> {
    if section.stringing.clipped() {
        return Ok(None);
    }
    let section = &*line.section_with_fractions(section);
    let cable = &*line.bimetallic.apply(cable);
    let cable = &*strung_cable(section, cable)?;
    let prepared = memo::prepared(line, section, cable, budget)?;
    if let Some(w) = &prepared.worked_back {
        return Ok(Some(w.clone()));
    }
    let clamps: Vec<Vector3<f64>> = prepared.layout.iter().map(plumb).collect();
    let target = asked_tension(section, &clamps, cable)?;
    Ok(Some(WorkedBack {
        condition: StrungAt::Initial,
        temperature: section.stringing.temperature(),
        target,
        initial: target,
        solves: 0,
        note: None,
    }))
}

/// What a report says of the sections strung to a value in a final
/// condition or by catenary constant, one line each: "section S1: sagged
/// to 22.00 kN after creep at 15 °C: 23.31 kN initial" — the initial
/// tension is the one the loads on the structures follow from; a section
/// whose stringing cannot be worked back says why. Nothing for the
/// sections strung to an initial tension or sag, or clipped in.
///
/// Expected cost: [`worked_back`] per such section (from the memo after a
/// solve).
pub fn final_stringing_notes(
    line: &Line,
    library: &ohl_cables::Library,
    budget: &Budget,
) -> Vec<String> {
    line.sections
        .iter()
        .filter(|s| s.stringing.since_version_15() && !s.stringing.clipped())
        .map(|s| {
            let found = library
                .get(&s.cable)
                .ok_or_else(|| format!("section {}: no wire type {}", s.id, s.cable))
                .and_then(|c| worked_back(line, s, c, budget).map_err(|e| e.to_string()));
            match found {
                Ok(Some(w)) => {
                    let constant = match &s.stringing {
                        Stringing::CatenaryConstant { constant, .. } => {
                            format!(" (catenary constant {constant} m)")
                        }
                        _ => String::new(),
                    };
                    format!("section {}: {}{constant}", s.id, w.text())
                }
                Ok(None) => format!("section {}: clipped in", s.id),
                // The error names its section itself.
                Err(e) => e,
            }
        })
        .collect()
}
