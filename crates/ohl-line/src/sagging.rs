// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The sagging table of a section ([`ohl_sagtension::sagging`]): what the
//! stringing crew measures, per span, in the stringing state (initial,
//! bare, still air) at the conductor temperature of sagging.
//!
//! The section is the line's spans between the clamps hanging in still
//! air at stringing, strung with the section's unstressed length (every
//! span at the stringing tension), in the blocks as the line's
//! calculation options say ([`crate::InTheBlocks`]): over frictionless
//! sheaves, the wire's tension equal on both sides of each (the default,
//! owner decision 2026-10-25: each span its own horizontal tension), or
//! simplified, one horizontal tension through the section.
//! Post-processing only.

use ohl_cables::Cable;
use ohl_cables::mechanics::{MaterialState, Mechanics};
use ohl_core::budget::Budget;
use ohl_core::units::{Force, Length, Temperature};
use ohl_sagtension::nonlinear::NonlinearSection;
use ohl_sagtension::sagging;
use ohl_weather::WeatherCase;

use crate::{Line, LineError, Section};

/// What the sagging table is asked for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SaggingOptions {
    /// Conductor temperature at sagging, °C (none: the stringing
    /// temperature).
    pub temperature: Option<f64>,
    /// Temperature at clipping in, °C, for the clipping offsets (none: the
    /// sagging temperature, where they are zero: the wire is clipped in
    /// as it lies in the blocks).
    pub clip_temperature: Option<f64>,
    /// Waves timed by the stopwatch.
    pub waves: u32,
    /// A transit at each span's back support, this far below it, m (none:
    /// no transit).
    pub transit_below_back: Option<f64>,
}

impl Default for SaggingOptions {
    fn default() -> Self {
        Self {
            temperature: None,
            clip_temperature: None,
            waves: 3,
            transit_below_back: None,
        }
    }
}

/// One span of the sagging table.
#[derive(Debug, Clone, PartialEq)]
pub struct SaggingSpan {
    /// Horizontal span, m.
    pub span: f64,
    /// Rise of the ahead clamp over the back one, m.
    pub rise: f64,
    /// Its horizontal tension at sagging, N (over sheaves each span's own).
    pub horizontal_tension: f64,
    /// Sag (vertical, where the wire is parallel to the chord), m.
    pub sag: f64,
    /// A level span's sag of the same span, tension and load, m.
    pub level_sag: f64,
    /// `sag - level_sag`, m.
    pub inclined_correction: f64,
    /// Sag change per kelvin, m/K.
    pub sag_per_kelvin: f64,
    /// Dynamometer at the back clamp, N.
    pub dyno_back: f64,
    /// Dynamometer at the ahead clamp, N.
    pub dyno_ahead: f64,
    /// Stopwatch: the return time of the waves, s.
    pub stopwatch: f64,
    /// Transit at the back support: the angle of sight (degrees above the
    /// horizontal) and the target's distance below the ahead clamp, m; or
    /// why there is none.
    pub transit: Option<Result<(f64, f64), String>>,
    /// Clipping offset at the ahead structure, m along the unstressed wire
    /// (positive: into the next span; zero at the far dead-end).
    pub clip_offset: f64,
}

/// The sagging table of a section.
#[derive(Debug, Clone, PartialEq)]
pub struct SaggingTable {
    /// Section id.
    pub section: String,
    /// Conductor temperature at sagging, °C.
    pub temperature: f64,
    /// Temperature at clipping in, °C.
    pub clip_temperature: f64,
    /// Horizontal tension at sagging, N: the spans' (equal H), or their mean
    /// weighted by the spans' lengths (over sheaves; each span's in
    /// [`SaggingSpan::horizontal_tension`]).
    pub horizontal_tension: f64,
    /// How the section lies in the blocks.
    pub blocks: crate::InTheBlocks,
    /// Per span, in line order.
    pub spans: Vec<SaggingSpan>,
    /// Warnings: states beyond the stress-strain curves' range.
    pub warnings: Vec<String>,
}

/// The sagging table of `section` strung with `cable`.
///
/// Expected cost: one still-air finite-element solve (the clamps at
/// stringing) and five section solves (~1 ms each for 10 spans in the
/// equal-H model, 10–20 times that over sheaves).
pub fn sagging_table(
    line: &Line,
    section: &Section,
    cable: &Cable,
    options: &SaggingOptions,
    budget: &Budget,
) -> Result<SaggingTable, LineError> {
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    let st = |e: ohl_sagtension::SagTensionError| invalid(e.to_string());
    let cable = &*line.bimetallic.apply(cable);
    let cable = &*crate::solve::strung_cable(section, cable)?;
    let (spans, _, strung_state, h) = crate::solve::strung(line, section, cable, budget)?;
    let mechanics = Mechanics::from_cable(cable).map_err(|e| invalid(e.to_string()))?;
    let ns = NonlinearSection::from_known_state(
        spans,
        mechanics,
        strung_state,
        Force::from_si(h),
        &MaterialState::Initial,
    )
    .map_err(st)?;
    let t = options
        .temperature
        .unwrap_or(strung_state.temperature.celsius());
    let clip_t = options.clip_temperature.unwrap_or(t);
    let bare = |t: f64| {
        crate::solve::load_state(
            &WeatherCase::still("sagging", Temperature::from_celsius(t)),
            cable,
        )
    };
    let blocks = line.solver.blocks;
    let solve = |t: f64| {
        match blocks {
            crate::InTheBlocks::Sheaves => {
                ns.solve_over_sheaves(bare(t), &MaterialState::Initial, budget)
            }
            crate::InTheBlocks::EqualH => ns.solve(bare(t), &MaterialState::Initial, budget),
        }
        .map_err(st)
    };
    let solved = solve(t)?;
    let dt = 0.5;
    let per_k = sagging::temperature_correction(&solve(t - dt)?, &solve(t + dt)?, dt);
    let clipped = solve(clip_t)?;
    let lengths = |sol, t| {
        ns.span_unstressed_lengths(sol, &bare(t), &MaterialState::Initial)
            .map_err(st)
    };
    let offsets = sagging::clipping_offsets(&lengths(&solved, t)?, &lengths(&clipped, clip_t)?);
    let warnings = ns
        .range_warnings(&solved, &bare(t), &MaterialState::Initial)
        .map_err(st)?;
    let waves = options.waves.max(1);
    let rows = solved
        .spans
        .iter()
        .enumerate()
        .map(|(k, sp)| SaggingSpan {
            span: sp.geometry.span.si(),
            rise: sp.geometry.rise.si(),
            horizontal_tension: sp.catenary.horizontal_tension.si(),
            sag: sp.sag().si(),
            level_sag: sagging::level_sag(sp).si(),
            inclined_correction: sagging::inclined_correction(sp).si(),
            sag_per_kelvin: per_k[k],
            dyno_back: sagging::dynamometer(sp, true).si(),
            dyno_ahead: sagging::dynamometer(sp, false).si(),
            stopwatch: sagging::stopwatch_time(sp, waves),
            transit: options.transit_below_back.map(|below| {
                sagging::angle_of_sight(sp, Length::ZERO, Length::from_si(-below), sp.geometry.span)
                    .map(|s| {
                        (
                            s.angle.to_degrees(),
                            (sp.geometry.rise - s.target_elevation).si(),
                        )
                    })
                    .map_err(|e| e.to_string())
            }),
            clip_offset: offsets[k],
        })
        .collect();
    Ok(SaggingTable {
        section: section.id.clone(),
        temperature: t,
        clip_temperature: clip_t,
        horizontal_tension: solved.horizontal_tension.si(),
        blocks,
        spans: rows,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_wirefe::default_fe_budget;
    use std::path::Path;

    #[test]
    fn sagging_table_of_the_example_line() {
        let line = crate::tests::example();
        let library = ohl_cables::Library::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap();
        let section = &line.sections[0];
        let cable = library.get(&section.cable).unwrap();
        let budget = default_fe_budget();
        let at = |options| sagging_table(&line, section, cable, &options, &budget).unwrap();
        let same = at(SaggingOptions {
            transit_below_back: Some(3.0),
            ..SaggingOptions::default()
        });
        assert!(!same.spans.is_empty());
        for s in &same.spans {
            // Clipped at the sagging temperature: no offsets (clipped in as
            // it lies in the blocks).
            assert!(s.clip_offset.abs() < 1e-9, "{}", s.clip_offset);
            assert!((s.inclined_correction - (s.sag - s.level_sag)).abs() < 1e-12);
            assert!(s.sag_per_kelvin > 0.0 && s.stopwatch > 0.0);
            assert!(s.transit.is_some());
        }
        // Clipped in warmer: offsets, ending at zero at the far dead-end.
        let warm = at(SaggingOptions {
            clip_temperature: Some(same.temperature + 20.0),
            ..SaggingOptions::default()
        });
        let last = warm.spans.last().unwrap().clip_offset;
        assert!(last.abs() < 1e-9, "{last}");
        if warm.spans.len() > 1 {
            assert!(warm.spans[0].clip_offset.abs() > 1e-6);
        }
    }
}
