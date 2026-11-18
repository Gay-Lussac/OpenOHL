// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Where the bolts of a member end may go, by the design code of the
//! model's capacity method (owner 2026-11-03: "Can the tool to add bolts
//! or move them on joints or members actually take the codes into account
//! and either snap on existing lines or offer to create a new one that
//! respect the norm? … And if the code changes that it adapts to the
//! code"): the code's minimum end distance, spacing and edge distance —
//! since 2026-11-18 every design code's are its plugin's (`bolt-distances`:
//! ASCE 10-97 4.5 in the bundled plugin `asce10`, SIA 263 Table 17 in
//! `sia263`), asked by `ohl_member_check::joints::bolt_rules` from what
//! [`basis`] works out here; the simple check has none ([`rules`]: none
//! in the core) — the bolts that break them ([`breaches`]) and where a
//! bolt asked for may go ([`snap`]: on a gauge line, or a new line
//! offered). The structure application's bolt tools and layout form use
//! these, so a change of capacity method changes what they allow.
//!
//! Without an analysis the force one bolt transmits is the plugin's own
//! (third round, 2026-11-18: its `bolt-distances-full` works out the
//! bolt's design shear capacity by its code from the bolts' data [`basis`]
//! sends). Where no code gives minimums (the simple check, a plugin
//! without minimum distances, one that returns none for the bolt) the
//! tools keep the structure's **default minimums** ([`assumed`]: end and
//! edge 1.5, spacing 3 bolt diameters to start with,
//! `[default_bolt_minimums]`) — "default minimum, assumed, not a code's",
//! never in place of a code's own.
//!
//! Bolt places are as the connections give them (`bolt_points`): along
//! the member from the joint's work point, across the bolted leg from the
//! heel, m. The end distance is taken from where the steel stops (the
//! end's cut).

use crate::design::MemberClass;
use crate::model::Model;

/// Where the force one bolt transmits comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Force {
    /// The largest member force of the last analysis over the bolts.
    Analysis(f64),
    /// The bolt's design shear capacity (no analysis: on the safe side).
    Capacity(f64),
    /// Not known (no analysis, no bolt capacity): the force terms left out.
    Unknown,
}

impl Force {
    /// The force, N.
    pub fn value(self) -> Option<f64> {
        match self {
            Force::Analysis(p) | Force::Capacity(p) => Some(p),
            Force::Unknown => None,
        }
    }

    /// In words, for the tools.
    pub fn words(self) -> String {
        match self {
            Force::Analysis(p) => format!("P {:.1} kN per bolt from the analysis", p / 1000.0),
            Force::Capacity(p) => {
                format!(
                    "P {:.1} kN per bolt: its shear capacity (no analysis)",
                    p / 1000.0
                )
            }
            Force::Unknown => "P not known: the force terms left out".into(),
        }
    }
}

/// The code's minimums at one member end, m (or, [`BoltRules::assumed`],
/// the structure's default minimums where no code gives any).
#[derive(Debug, Clone, PartialEq)]
pub struct BoltRules {
    /// The code (as the results name it; a plugin's method: its name);
    /// the default minimums: [`ohl_structures::bolts::DEFAULT_MINIMUM_WORDS`].
    pub code: String,
    /// Minimum end distance, from where the steel stops to a bolt.
    pub end: f64,
    /// Minimum distance between two bolts.
    pub spacing: f64,
    /// Minimum distance from a bolt to the leg's free edge (its toe).
    pub edge: f64,
    /// The clauses, for the words: end, spacing, edge.
    pub clauses: [String; 3],
    /// The force per bolt used.
    pub force: Force,
    /// Where a [`Force::Capacity`] comes from, in words (the plugin's:
    /// "the bolt's design shear capacity, ASCE 10-97 4.3.2"; "" not said).
    pub force_basis: String,
    /// The bolts' diameter, m.
    pub diameter: f64,
    /// The bolted leg's width (heel to toe) and thickness, m (none: not
    /// an angle).
    pub leg: Option<(f64, f64)>,
    /// The leg's root radius, m.
    pub root: f64,
    /// The edge taken as sheared.
    pub sheared: bool,
    /// Not a code's: the structure's default minimums ([`assumed`]), and
    /// why no code's ("the simple check gives no minimum distances", "no
    /// minimum distances from sia263: …"). None: the code's.
    pub assumed: Option<String>,
}

impl BoltRules {
    /// The minimums in one line: "ASCE 10-97 4.5: end ≥ 31 mm, spacing ≥
    /// 52 mm, edge ≥ 28 mm (sheared edge; P …)"; the default minimums
    /// "default minimum, assumed, not a code's: end ≥ 24 mm (1.5 d), … —
    /// why".
    pub fn words(&self) -> String {
        if let Some(why) = &self.assumed {
            return format!(
                "{}: end ≥ {:.0} mm ({}), spacing ≥ {:.0} mm ({}), edge ≥ {:.0} mm ({}) — {why}",
                self.code,
                self.end * 1000.0,
                self.clauses[0],
                self.spacing * 1000.0,
                self.clauses[1],
                self.edge * 1000.0,
                self.clauses[2],
            );
        }
        format!(
            "{}: end ≥ {:.0} mm ({}), spacing ≥ {:.0} mm ({}), edge ≥ {:.0} mm ({}; {} edge) — {}",
            self.code,
            self.end * 1000.0,
            self.clauses[0],
            self.spacing * 1000.0,
            self.clauses[1],
            self.edge * 1000.0,
            self.clauses[2],
            if self.sheared { "sheared" } else { "rolled" },
            self.force_words()
        )
    }

    /// The force per bolt in words, with where a capacity comes from.
    pub fn force_words(&self) -> String {
        match self.force {
            Force::Capacity(p) if !self.force_basis.is_empty() => format!(
                "P {:.1} kN per bolt: {} (no analysis)",
                p / 1000.0,
                self.force_basis
            ),
            f => f.words(),
        }
    }

    /// The smallest gauge from the heel a bolt line may take: its hole
    /// clear of the root fillet (geometry, not a code rule), m.
    pub fn least_gauge(&self) -> f64 {
        let t = self.leg.map_or(0.0, |l| l.1);
        t + self.root + self.diameter / 2.0
    }
}

/// The code's minimums for the bolts of member `idx` at its end `end` (0
/// at its first joint), `force`: the largest axial force over the
/// analysis' cases, N (none: no analysis). None from the core since
/// 2026-11-18: every design code's minimums are its plugin's
/// (`ohl_member_check::joints::bolt_rules` asks it, from [`basis`]), and
/// the simple check has none — never another code's in their place.
pub fn rules(model: &Model, idx: usize, end: usize, force: Option<f64>) -> Option<BoltRules> {
    rules_in(model, idx, end, force, None)
}

/// [`rules`] in another connected part than the member's leg (owner
/// 2026-11-03: a plate's edges by the plate's own thickness and steel):
/// `part` its thickness, m, and its ultimate strength, Pa (none: the
/// member's). None from the core (see [`rules`]).
pub fn rules_in(
    _model: &Model,
    _idx: usize,
    _end: usize,
    _force: Option<f64>,
    _part: Option<(f64, Option<f64>)>,
) -> Option<BoltRules> {
    None
}

/// The structure's default minimums for the bolts of member `idx`'s end
/// `end` where no code gives any (owner 2026-11-18, third round: the
/// simple check, a plugin without minimum distances, one that returns
/// none): [`Model::bolt_defaults`] times the bolts' diameter, said as
/// "default minimum, assumed, not a code's" with `why` (no code's). `part`
/// as [`rules_in`]'s (the leg's width only matters). None: the end has no
/// bolts of known diameter. Never in place of a code's own minimums.
pub fn assumed(
    model: &Model,
    idx: usize,
    end: usize,
    part: Option<(f64, Option<f64>)>,
    why: String,
) -> Option<BoltRules> {
    let b = basis(model, idx, end, None, part)?;
    let [e, s, g] = model.bolt_defaults.of(b.diameter);
    Some(BoltRules {
        code: ohl_structures::bolts::DEFAULT_MINIMUM_WORDS.to_owned(),
        end: e,
        spacing: s,
        edge: g,
        clauses: model.bolt_defaults.clauses(),
        force: Force::Unknown,
        force_basis: String::new(),
        diameter: b.diameter,
        leg: b.leg,
        root: b.root,
        sheared: b.sheared,
        assumed: Some(why),
    })
}

/// The bolts of a member end as the minimum distances' force basis takes
/// them: the end's connection over the member's design data
/// ([`crate::member_ends::end_bolts`]), what a design code's plugin works
/// out the bolt's own shear capacity from (third round, 2026-11-18).
#[derive(Debug, Clone, PartialEq)]
pub struct EndBolts {
    /// How many bolts.
    pub count: u32,
    /// Shear planes per bolt.
    pub shear_planes: u32,
    /// One bolt's shear capacity per plane through the threads, N (given).
    pub shear_capacity: Option<f64>,
    /// The same through the shank, N.
    pub shear_capacity_shank: Option<f64>,
    /// The bolt's ultimate strength, Pa.
    pub ultimate: Option<f64>,
    /// The threads are in the shear plane (none: not said).
    pub threads_in_shear: Option<bool>,
    /// Its grade as written ("8.8"; none: not given).
    pub grade: Option<String>,
}

/// What a code's minimum distances at a member end are worked out from:
/// the data of the end, whatever the code (a plugin's method through its
/// runner).
#[derive(Debug, Clone, PartialEq)]
pub struct Basis {
    /// The bolts' diameter, m.
    pub diameter: f64,
    /// The connected part's thickness, m (0: not known).
    pub thickness: f64,
    /// The connected part's ultimate strength, Pa.
    pub ultimate: Option<f64>,
    /// The force per bolt by the analysis, N (none: no analysis, or no
    /// bolts counted) — without it the plugin takes its own.
    pub analysis: Option<f64>,
    /// The end's bolts (none: only their diameter is known).
    pub bolts: Option<EndBolts>,
    /// The bolted leg's width and thickness, m (none: not an angle).
    pub leg: Option<(f64, f64)>,
    /// The leg's root radius, m.
    pub root: f64,
    /// The edge taken as sheared.
    pub sheared: bool,
    /// The member is a redundant.
    pub redundant: bool,
}

/// The [`Basis`] of member `idx`'s end `end` (arguments as
/// [`rules_in`]); none: the end has no bolts of known diameter.
pub fn basis(
    model: &Model,
    idx: usize,
    end: usize,
    force: Option<f64>,
    part: Option<(f64, Option<f64>)>,
) -> Option<Basis> {
    let m = model.members.get(idx)?;
    let d = &m.design;
    let e = d.ends.get(end).and_then(Option::as_ref);
    let s = &model.sections[m.section];
    // The bolts as the checks merge them (their diameter and count as
    // before the third round: the merged bolts', else the end's own of
    // known diameter, else the member's).
    let (diameter, count, bolts) = match crate::member_ends::end_bolts(d, e, &mut Vec::new()) {
        Some((b, shank, threads)) => (
            b.diameter,
            b.count,
            Some(EndBolts {
                count: b.count,
                shear_planes: b.shear_planes,
                shear_capacity: b.shear_capacity,
                shear_capacity_shank: shank,
                ultimate: b.ultimate,
                threads_in_shear: threads,
                grade: b.grade.map(|g| format!("{}.{}", g.major, g.minor)),
            }),
        ),
        None => {
            let b = e
                .and_then(|e| e.bolts)
                .and_then(|b| b.diameter.map(|dia| (dia, b.count)));
            let (dia, n) = b.or(d.bolts.map(|b| (b.diameter, b.count)))?;
            (dia, n, None)
        }
    };
    let (leg, root) = match s.profile.as_ref().map(|p| p.shape.clone()) {
        Some(ohl_sections::Shape::EqualAngle { h, t, r }) => (Some((h, t)), r),
        Some(ohl_sections::Shape::UnequalAngle { h, b, t, r }) => {
            let leg = e.and_then(|e| e.leg).or(d.connected_leg);
            let w = if leg == Some(crate::design::ConnectedLeg::Short) {
                h.min(b)
            } else {
                h.max(b)
            };
            (Some((w, t)), r)
        }
        _ => (None, 0.0),
    };
    let t = match part {
        Some((t, _)) => t,
        None => d.thickness.or(leg.map(|l| l.1)).unwrap_or(0.0),
    };
    let analysis = force
        .filter(|_| count > 0)
        .map(|n| n.abs() / f64::from(count));
    let sheared = !e.and_then(|e| e.rolled_edge).unwrap_or(false);
    Some(Basis {
        diameter,
        thickness: t,
        ultimate: part.and_then(|p| p.1).or(s.ultimate),
        analysis,
        bolts,
        leg,
        root,
        sheared,
        redundant: d.class == Some(MemberClass::Redundant),
    })
}

/// The force basis of a plugin that works out none itself (one built
/// against `bolt-distances` alone, without `bolt-distances-full`): the
/// bolt's shear capacity as ASCE 10-97 4.3.2 gives it
/// ([`crate::member_ends::bolt_shear_per_bolt`]), sent as the force per
/// bolt so that such a plugin answers as it did before the third round —
/// the only use of that function left (2026-11-18). N; none: no bolts or
/// no capacity.
pub fn shear_without_plugin(model: &Model, idx: usize, end: usize) -> Option<f64> {
    let d = &model.members.get(idx)?.design;
    let e = d.ends.get(end).and_then(Option::as_ref);
    crate::member_ends::bolt_shear_per_bolt(d, e).map(|x| x.0)
}

/// A bolt that breaks a rule, in words with the clause.
#[derive(Debug, Clone, PartialEq)]
pub struct Breach {
    /// The bolt (its place in the list checked).
    pub bolt: usize,
    /// What it breaks: "end distance 25 mm below 31 mm (ASCE 10-97 4.5.1)".
    pub what: String,
}

/// The bolts of `points` (along from the work point, across from the
/// heel) that break `r`, the steel stopping `cut` m from the work point:
/// the end distance, the spacing to every other bolt (taken between any
/// two, also on different gauge lines — on the safe side: the code's is
/// along a line of force), the edge distance to the toe.
pub fn breaches(r: &BoltRules, points: &[[f64; 2]], cut: f64) -> Vec<Breach> {
    let mm = |x: f64| x * 1000.0;
    let mut out = Vec::new();
    for (i, p) in points.iter().enumerate() {
        let e = p[0] - cut;
        if e < r.end - 1e-6 {
            out.push(Breach {
                bolt: i,
                what: format!(
                    "end distance {:.0} mm below {:.0} mm ({} {})",
                    mm(e),
                    mm(r.end),
                    r.code,
                    r.clauses[0]
                ),
            });
        }
        if let Some((w, _)) = r.leg {
            let f = w - p[1];
            if f < r.edge - 1e-6 {
                out.push(Breach {
                    bolt: i,
                    what: format!(
                        "edge distance {:.0} mm below {:.0} mm ({} {})",
                        mm(f),
                        mm(r.edge),
                        r.code,
                        r.clauses[2]
                    ),
                });
            }
        }
        for (k, q) in points.iter().enumerate().take(i) {
            let s = (p[0] - q[0]).hypot(p[1] - q[1]);
            if s < r.spacing - 1e-6 {
                out.push(Breach {
                    bolt: i,
                    what: format!(
                        "{:.0} mm from bolt {}, below {:.0} mm ({} {})",
                        mm(s),
                        k + 1,
                        mm(r.spacing),
                        r.code,
                        r.clauses[1]
                    ),
                });
            }
        }
    }
    out
}

/// Where a bolt asked for may go.
#[derive(Debug, Clone, PartialEq)]
pub enum Snap {
    /// On an existing gauge line, at this place.
    OnLine([f64; 2]),
    /// No room on the lines near it: a new gauge line offered at this
    /// gauge, the bolt at this place (to be confirmed).
    NewLine {
        /// The new line's gauge from the heel, m.
        gauge: f64,
        /// The bolt's place.
        at: [f64; 2],
    },
    /// Nowhere that keeps the rules (the leg full, or three lines
    /// already): why.
    Nowhere(String),
}

/// The most gauge lines a leg takes.
const MAX_LINES: usize = ohl_structures::connections::MAX_GAUGE_LINES as usize;

/// Where a bolt asked for at `wanted` (along from the work point, across
/// from the heel; `across` none: the view does not show it — the nearest
/// line) may go among the other bolts `others`, on the gauge lines
/// `lines` (from the heel), the steel stopping `cut` m from the work
/// point:
///
/// 1. on the gauge line nearest the place asked (within half the
///    spacing), along it at the place asked or the nearest place that
///    keeps the end distance and the spacing to the other bolts;
/// 2. else a new gauge line, if there are fewer than three, at the gauge
///    nearest the place asked that keeps the edge distance to the toe,
///    the spacing to the other bolts and clears the root fillet;
/// 3. else nowhere.
pub fn snap(
    r: &BoltRules,
    others: &[[f64; 2]],
    lines: &[f64],
    cut: f64,
    wanted: [f64; 2],
    across_known: bool,
) -> Snap {
    let first = cut + r.end;
    let fits = |p: [f64; 2]| {
        p[0] >= first - 1e-9
            && r.leg.is_none_or(|(w, _)| w - p[1] >= r.edge - 1e-9)
            && others
                .iter()
                .all(|q| (p[0] - q[0]).hypot(p[1] - q[1]) >= r.spacing - 1e-9)
    };
    // Along a line at `g`: the place asked if free, else the nearest free
    // place, in 1 mm steps up to five spacings away.
    let along_line = |g: f64| -> Option<[f64; 2]> {
        let start = wanted[0].max(first);
        (0..=(5.0 * r.spacing / 0.001) as usize)
            .flat_map(|k| {
                let d = k as f64 * 0.001;
                [start + d, start - d]
            })
            .map(|a| [a, g])
            .find(|p| fits(*p))
    };
    let mut known: Vec<f64> = lines.to_vec();
    known.sort_by(f64::total_cmp);
    known.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    let nearest = known
        .iter()
        .copied()
        .min_by(|x, y| (x - wanted[1]).abs().total_cmp(&(y - wanted[1]).abs()));
    if let Some(g) = nearest
        && (!across_known || (g - wanted[1]).abs() <= r.spacing / 2.0)
        && let Some(p) = along_line(g)
        && (p[0] - wanted[0].max(first)).abs() <= r.spacing
    {
        return Snap::OnLine(p);
    }
    if known.len() >= MAX_LINES {
        return Snap::Nowhere(format!(
            "no room within one spacing on the gauge lines, and {MAX_LINES} lines already"
        ));
    }
    let Some((w, _)) = r.leg else {
        return match nearest.and_then(along_line) {
            Some(p) => Snap::OnLine(p),
            None => Snap::Nowhere("no room on the gauge line".into()),
        };
    };
    // A new line: the gauge nearest the place asked (or the middle of the
    // free room) in 1 mm steps between the fillet and the toe's edge
    // distance.
    let (lo, hi) = (r.least_gauge(), w - r.edge);
    if lo > hi {
        return Snap::Nowhere(format!(
            "the leg ({:.0} mm) has no room for a line: fillet {:.0} mm, edge {:.0} mm",
            w * 1000.0,
            lo * 1000.0,
            r.edge * 1000.0
        ));
    }
    let aim = if across_known {
        wanted[1]
    } else {
        (lo + hi) / 2.0
    };
    let aim = aim.clamp(lo, hi);
    let steps = ((hi - lo) / 0.001) as usize + 1;
    let along = wanted[0].max(first);
    let mut tries: Vec<f64> = (0..steps).map(|k| lo + k as f64 * 0.001).collect();
    tries.sort_by(|x, y| (x - aim).abs().total_cmp(&(y - aim).abs()));
    for g in tries {
        if known.iter().any(|l| (l - g).abs() < r.diameter) {
            continue;
        }
        let p = [along, g];
        if fits(p) {
            return Snap::NewLine {
                gauge: (g * 1e4).round() / 1e4,
                at: p,
            };
        }
    }
    Snap::Nowhere("no gauge on the leg keeps the spacing and edge distance here".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> BoltRules {
        BoltRules {
            code: "ASCE 10-97".into(),
            end: 0.025,
            spacing: 0.05,
            edge: 0.025,
            clauses: ["4.5.1", "4.5.2", "4.5.3"].map(str::to_owned),
            force: Force::Unknown,
            force_basis: String::new(),
            diameter: 0.016,
            leg: Some((0.1, 0.008)),
            root: 0.01,
            sheared: true,
            assumed: None,
        }
    }

    /// Breaches: a bolt 20 mm from the steel's end, one 40 mm from it,
    /// one 20 mm from the toe — each named with its clause.
    #[test]
    fn breaches_name_the_clause() {
        let r = rules();
        let b = breaches(&r, &[[0.03, 0.05], [0.07, 0.05], [0.12, 0.08]], 0.01);
        let words: Vec<&str> = b.iter().map(|b| b.what.as_str()).collect();
        assert!(words[0].starts_with("end distance 20 mm below 25 mm (ASCE 10-97 4.5.1)"));
        assert!(words.iter().any(|w| w.starts_with("40 mm from bolt 1")));
        assert!(
            words
                .iter()
                .any(|w| w.starts_with("edge distance 20 mm below 25"))
        );
        assert!(breaches(&r, &[[0.035, 0.05], [0.085, 0.05]], 0.01).is_empty());
    }

    /// Snapping: onto the line, pushed along to keep the spacing; a new
    /// line offered where the line has no room near the place asked; none
    /// with three lines full.
    #[test]
    fn snap_on_a_line_or_a_new_one() {
        // A 150 mm leg: room for a line 43.3 mm (√(50² − 25²)) beside.
        let r = BoltRules {
            leg: Some((0.15, 0.008)),
            ..rules()
        };
        let others = [[0.035, 0.04], [0.085, 0.04]];
        // Asked between the two, near the line: no room on it (they are
        // 50 mm apart): a new line, at least 43.3 mm from it.
        match snap(&r, &others, &[0.04], 0.01, [0.06, 0.045], true) {
            Snap::NewLine { gauge, at } => {
                assert!((0.0833..=0.125 + 1e-9).contains(&gauge), "{gauge}");
                assert!((at[0] - 0.06).abs() < 1e-9);
                assert!(((at[0] - 0.035).hypot(gauge - 0.04)) >= 0.05 - 1e-9);
            }
            s => panic!("{s:?}"),
        }
        // Asked beyond the last bolt: on the line, at least a spacing on.
        assert_eq!(
            snap(&r, &others, &[0.04], 0.01, [0.12, 0.05], true),
            Snap::OnLine([0.135, 0.04])
        );
        // Too near the steel's end: moved to the end distance.
        assert_eq!(
            snap(&r, &[], &[0.04], 0.01, [0.02, 0.04], true),
            Snap::OnLine([0.035, 0.04])
        );
        // Three lines, each with bolts a spacing apart: no room within a
        // spacing of the place asked, no fourth line: nowhere.
        let full = [0.03, 0.05, 0.07];
        let crowd: Vec<[f64; 2]> = full
            .iter()
            .flat_map(|g| [0.035, 0.085, 0.135, 0.185].map(|a| [a, *g]))
            .collect();
        assert!(matches!(
            snap(&r, &crowd, &full, 0.01, [0.06, 0.05], true),
            Snap::Nowhere(_)
        ));
    }
}
