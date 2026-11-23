// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Solve the clashes** (O44 L11, the owner, 2026-11-21: "all the errors in
//! a menu that can be opened and the automatic layout should find the best
//! solution to offer to solve all of it and that can be put in place with a
//! simple click"): one joint's best whole solution, worked out on copies of
//! the structure and weighed, for Joint details' window of that name.
//!
//! Tried, in this order (each on a copy; nothing written):
//!
//! 1. the layout of the joint ([`crate::lay_out`]), values given by hand
//!    kept; then with them replaced;
//! 2. where the layout proposes pieces in line one part or a leg splice
//!    place at the joint (J9, J7: never written by a run), the same with
//!    those confirmed — **marked** as a confirmation in the solution's
//!    effects, never silent;
//! 3. on the best so far, the fixes of the joint's problems that are left
//!    ([`crate::joint_fixes`]), each kept only where the joint has fewer
//!    problems after it, one at a time.
//!
//! Each is weighed by the clashes at the joint, then its other problems
//! (fewer is better), and **refused** where it pulls back a leg or a member
//! running on through the joint, turns a leg, changes a member's section, or
//! leaves more clashes and problems at the joints of the members it touches
//! (their other ends). The best that is strictly better than the joint as it
//! is is the solution; else none, with why. A leg piece the joints say is
//! turned ([`crate::leg_roll`]) and (O44 L18) a leg's or chord's run turned
//! from its joints' planes ([`crate::plane_rolls`]) are only **proposed**,
//! side by side, each with the solution worked out again with that turn —
//! every joint the turned members reach laid out with it and checked, the
//! turn refused where those joints are left with more clashes and problems
//! (its effect measured over them all) — never applied unless asked.

use std::collections::{BTreeMap, BTreeSet};

use ohl_sections::SectionLibrary;
use ohl_structures::bolts::BoltLibrary;
use ohl_structures::{LibraryItem, StructureType};
use toml_edit::{DocumentMut, Table};

use crate::joints::{JointDetail, JointDetails, Problem};
use crate::layout_run::{LayoutRun, RunAsk};
use crate::{BoltArea, LayoutSettings};

/// What to solve.
#[derive(Debug, Clone)]
pub struct SolveAsk<'a> {
    /// The section library.
    pub library: &'a SectionLibrary,
    /// The bolt library.
    pub bolts: &'a BoltLibrary,
    /// The layout's settings.
    pub settings: &'a LayoutSettings,
    /// The presets' fields by class ([`RunAsk::preset_fields`]).
    pub preset_fields: BTreeMap<String, Table>,
    /// The provisional bolts of a leg splice confirmed (the preset for the
    /// class `leg`, else the bolt picked).
    pub leg_bolt: Option<BoltArea>,
    /// The joint.
    pub joint: String,
}

/// What is left at a joint, and at the joints around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// Clashes at the joint (pairs of parts in the same place).
    pub clashes: usize,
    /// The joint's other problems (member ends not fitting as given).
    pub problems: usize,
    /// Clashes and problems at the other joints of the members meeting
    /// there.
    pub around: usize,
    /// Members the layout leaves for a hand detail there (0 as it is: not
    /// laid out).
    pub left: usize,
}

impl Counts {
    fn key(&self) -> (usize, usize, usize) {
        (self.clashes, self.problems, self.left)
    }
}

/// What kind of change an effect is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    /// A proposal of the layout confirmed (pieces one part, a splice place).
    Confirmed,
    /// A member turned about its axis.
    Turned,
    /// Where a member's steel stops changed.
    Cut,
    /// A member end moved sideways (an eccentricity the analysis takes).
    Moved,
    /// What a member end bolts onto, or the face it sits on, changed.
    Onto,
    /// A plate added.
    PlateAdded,
    /// A plate taken away.
    PlateRemoved,
    /// A fix of a problem made.
    Fixed,
    /// A member left for a hand detail.
    Left,
}

/// One effect of a solution, in words.
#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    /// What kind.
    pub kind: EffectKind,
    /// In words.
    pub words: String,
}

impl Effect {
    /// It changes the steel or how it is analysed (a turn, a move, a
    /// confirmation): marked in the window.
    pub fn marked(&self) -> bool {
        matches!(
            self.kind,
            EffectKind::Confirmed | EffectKind::Turned | EffectKind::Moved
        )
    }
}

/// One way tried.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// What was tried.
    pub label: String,
    /// The structure file after it.
    pub text: String,
    /// What is left after it.
    pub counts: Counts,
    /// Why it is not taken whatever its counts (it pulls back a leg…).
    pub refused: Option<String>,
    /// What it changes at the joint.
    pub effects: Vec<Effect>,
    /// The members the layout left for a hand detail, with why.
    pub hand: Vec<String>,
}

/// Where a turn proposed comes from (O44 L18: both offered side by side).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnSource {
    /// What the ends bolting onto the piece at its joints say (the
    /// leg-rotation check, L11): the piece and the pieces in line with it.
    Joints,
    /// Its run turned from its joints' planes (L18,
    /// [`crate::plane_rolls`]): the whole leg or chord in line.
    Planes,
}

impl TurnSource {
    /// In words, short.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Joints => "from the ends at its joints (L11)",
            Self::Planes => "from its joints' planes (L18)",
        }
    }
}

/// A turn's effect over every joint the turned members reach (O44 L18, the
/// owner: "when turning a member both sides should be reverified").
#[derive(Debug, Clone, PartialEq)]
pub struct TurnReach {
    /// The joints (both ends of every member turned).
    pub joints: Vec<String>,
    /// Clashes and other problems at them as they are.
    pub before: usize,
    /// With the turn's solution.
    pub after: usize,
    /// Of them laid out whole with the turn (the layout's run of them all).
    pub whole: usize,
    /// Member ends laid out at them with the turn.
    pub ends: usize,
}

/// A leg or chord piece the joints say is turned: proposed, never applied
/// unless asked, with the solution worked out again with it.
#[derive(Debug, Clone, PartialEq)]
pub struct LegTurn {
    /// Where the proposal comes from.
    pub source: TurnSource,
    /// The turn, degrees (added to each member's roll).
    pub turn: f64,
    /// The proposal in words.
    pub words: String,
    /// What the joints say (the leg-rotation check's).
    pub roll: Option<crate::LegRoll>,
    /// The run turned from its joints' planes.
    pub plane: Option<crate::plane_rolls::PlaneRoll>,
    /// The members turned alike (the piece and the pieces in line with it;
    /// from the planes, the whole run).
    pub members: Vec<String>,
    /// The best solution with the turn (none: nothing better found).
    pub with: Option<Candidate>,
    /// Its effect over every joint the members reach (none: no solution).
    pub reach: Option<TurnReach>,
}

/// The solution of one joint.
#[derive(Debug, Clone, PartialEq)]
pub struct Solution {
    /// The joint.
    pub joint: String,
    /// What is there now.
    pub before: Counts,
    /// Every way tried, in order.
    pub tried: Vec<Candidate>,
    /// The one chosen (an index into `tried`); none: none better.
    pub chosen: Option<usize>,
    /// Why none is better (when none is chosen).
    pub why_none: Option<String>,
    /// The leg turns proposed.
    pub leg_turns: Vec<LegTurn>,
    /// How long it took, s.
    pub seconds: f64,
}

impl Solution {
    /// The solution chosen.
    pub fn best(&self) -> Option<&Candidate> {
        self.chosen.and_then(|k| self.tried.get(k))
    }
}

/// Fixes tried per round at most, and rounds (the program's bounds: a
/// joint's left-over problems after the layout are few).
const FIX_TRIES: usize = 24;
const FIX_ROUNDS: usize = 4;

/// The best whole solution of joint `ask.joint` of the structure file
/// `text` (see the module).
pub fn solve_joint(text: &str, ask: &SolveAsk<'_>) -> Result<Solution, String> {
    let start = std::time::Instant::now();
    let joint = ask.joint.clone();
    let t0 = parse(text)?;
    // The joint and the other joints of its members.
    let mut focus: BTreeSet<String> = BTreeSet::from([joint.clone()]);
    for m in t0.members.iter().filter(|m| m.nodes.contains(&joint)) {
        focus.extend(m.nodes.iter().cloned());
    }
    let (before, d0) = measure(text, ask, &focus, &joint)?;
    let j0 = d0
        .joint(&joint)
        .cloned()
        .ok_or_else(|| format!("no joint {joint}"))?;
    let mut tried: Vec<Candidate> = Vec::new();
    let allowed: BTreeSet<String> = BTreeSet::new();
    let judge = |label: String,
                 after: String,
                 confirms: &[String],
                 fixes: &[String],
                 run: Option<&LayoutRun>,
                 turned_ok: &BTreeSet<String>|
     -> Result<Candidate, String> {
        let (mut counts, d1) = measure(&after, ask, &focus, &joint)?;
        counts.left = run
            .and_then(|r| r.joints.iter().find(|o| o.joint == joint))
            .map_or(0, |o| o.plan.hand_members.len());
        let t1 = parse(&after)?;
        let j1 = d1.joint(&joint);
        let refused = refusal(&t0, &t1, &j0, j1, turned_ok).or_else(|| {
            (counts.around > before.around).then(|| {
                format!(
                    "more clashes and problems at the joints around ({} to {})",
                    before.around, counts.around
                )
            })
        });
        let mut effects: Vec<Effect> = confirms
            .iter()
            .map(|w| Effect {
                kind: EffectKind::Confirmed,
                words: w.clone(),
            })
            .collect();
        effects.extend(diff(&t0, &t1, &j0, j1));
        effects.extend(fixes.iter().map(|w| Effect {
            kind: EffectKind::Fixed,
            words: w.clone(),
        }));
        let hand: Vec<String> = run
            .and_then(|r| r.joints.iter().find(|o| o.joint == joint))
            .map(|o| o.hand.clone())
            .unwrap_or_default();
        effects.extend(hand.iter().map(|h| Effect {
            kind: EffectKind::Left,
            words: format!("left for a hand detail — {h}"),
        }));
        Ok(Candidate {
            label,
            text: after,
            counts,
            refused,
            effects,
            hand,
        })
    };
    let run_on =
        |text: &str, replace: bool, settings: &LayoutSettings| -> Result<LayoutRun, String> {
            crate::lay_out(
                text,
                &RunAsk {
                    library: ask.library,
                    bolts: ask.bolts,
                    settings,
                    preset_fields: ask.preset_fields.clone(),
                    joints: Some(BTreeSet::from([joint.clone()])),
                    replace_hand: replace,
                },
            )
        };
    // 1. The layout, values given by hand kept, then replaced; 2. the same
    // with the layout's proposals at the joint confirmed (marked).
    let bases = bases_of(text, ask, &t0, &joint)?;
    // (L11: an end moved sideways to pass the flange on a plate, off in a
    // run, tried here too where the layout leaves something.)
    let shifted = LayoutSettings {
        plate_shift: true,
        ..ask.settings.clone()
    };
    for b in &bases {
        for replace in [false, true] {
            let run = run_on(&b.text, replace, ask.settings)?;
            let mut left_some = false;
            if run.text != text {
                let label = format!(
                    "{}the layout of the joint{}",
                    b.prefix,
                    if replace {
                        ", values given by hand replaced"
                    } else {
                        ""
                    }
                );
                let c = judge(
                    label,
                    run.text.clone(),
                    &b.confirms,
                    &[],
                    Some(&run),
                    &allowed,
                )?;
                left_some = c.counts.clashes + c.counts.problems + c.counts.left > 0;
                tried.push(c);
            }
            if ask.settings.plate_shift || !(left_some || run.text == text) {
                continue;
            }
            let run2 = run_on(&b.text, replace, &shifted)?;
            if run2.text == text || run2.text == run.text {
                continue;
            }
            let label = format!(
                "{}the layout of the joint{}, ends moved sideways to pass the flange on a plate",
                b.prefix,
                if replace {
                    ", values given by hand replaced"
                } else {
                    ""
                }
            );
            tried.push(judge(
                label,
                run2.text.clone(),
                &b.confirms,
                &[],
                Some(&run2),
                &allowed,
            )?);
        }
    }
    // 3. The fixes of what is left, on the best so far (or as it is).
    let base = best_of(&tried, &before);
    let (base_text, base_counts, base_label, base_confirms, base_hand) = match base {
        Some(k) => (
            tried[k].text.clone(),
            tried[k].counts,
            tried[k].label.clone(),
            tried[k]
                .effects
                .iter()
                .filter(|e| e.kind == EffectKind::Confirmed)
                .map(|e| e.words.clone())
                .collect::<Vec<_>>(),
            tried[k].hand.clone(),
        ),
        None => (
            text.to_owned(),
            before,
            String::new(),
            Vec::new(),
            Vec::new(),
        ),
    };
    if base_counts.clashes + base_counts.problems > 0 {
        let (fixed, labels) = fix_rounds(&base_text, base_counts, ask, &focus, &joint, &t0, &j0)?;
        if !labels.is_empty() {
            let label = if base_label.is_empty() {
                "the fixes of the joint's problems".to_owned()
            } else {
                format!("{base_label}, then the fixes of what is left")
            };
            let mut c = judge(label, fixed, &base_confirms, &labels, None, &allowed)?;
            // (What the layout left by hand there stays so: said again.)
            c.counts.left = base_counts.left;
            c.effects.extend(base_hand.iter().map(|h| Effect {
                kind: EffectKind::Left,
                words: format!("left for a hand detail — {h}"),
            }));
            c.hand = base_hand;
            tried.push(c);
        }
    }
    let chosen = best_of(&tried, &before);
    let why_none = if chosen.is_some() {
        None
    } else {
        Some(why_none(&tried, &before))
    };
    // A leg piece at the joint the joints say is turned: proposed, the
    // solution worked out again with it.
    let leg_turns = leg_turns(text, ask, &t0, &j0, &bases, &focus)?;
    Ok(Solution {
        joint,
        before,
        tried,
        chosen,
        why_none,
        leg_turns,
        seconds: start.elapsed().as_secs_f64(),
    })
}

/// A starting point of the ways tried: the file as it is, or with the
/// layout's proposals at the joint confirmed.
struct Base {
    /// Said before the way ("the layout's proposals here confirmed, then ").
    prefix: &'static str,
    /// The file.
    text: String,
    /// The confirmations made, in words (marked).
    confirms: Vec<String>,
}

/// The starting points: `text` as it is; and where the layout proposes
/// pieces in line one part or a leg splice place at `joint` (J9, J7: never
/// written by a run), `text` with those confirmed.
fn bases_of(
    text: &str,
    ask: &SolveAsk<'_>,
    t0: &StructureType,
    joint: &str,
) -> Result<Vec<Base>, String> {
    let mut out = vec![Base {
        prefix: "",
        text: text.to_owned(),
        confirms: Vec::new(),
    }];
    let t = t0;
    let only = BTreeSet::from([joint.to_owned()]);
    let one: Vec<crate::InLine> = crate::in_line_pieces(t, Some(&only))
        .into_iter()
        .filter(|x| x.joint == joint && x.proposed())
        .collect();
    // (The leg splices as a run proposes them: from the joints as planned.)
    let run = crate::lay_out(
        text,
        &RunAsk {
            library: ask.library,
            bolts: ask.bolts,
            settings: ask.settings,
            preset_fields: ask.preset_fields.clone(),
            joints: Some(only.clone()),
            replace_hand: false,
        },
    )?;
    let spl: Vec<crate::LegSplice> = run
        .splices
        .iter()
        .filter(|s| s.joint == joint && s.proposed())
        .cloned()
        .collect();
    let one: Vec<crate::InLine> = if one.is_empty() {
        run.in_line
            .iter()
            .filter(|x| x.joint == joint && x.proposed())
            .cloned()
            .collect()
    } else {
        one
    };
    if one.is_empty() && spl.is_empty() {
        return Ok(out);
    }
    let words: Vec<String> = one
        .iter()
        .map(|x| {
            format!(
                "{} and {} confirmed one part (the layout's proposal: pieces in line{})",
                x.member,
                x.onto,
                if x.splice_bolts {
                    ", a leg of one section"
                } else {
                    ""
                }
            )
        })
        .chain(spl.iter().map(|s| {
            format!(
                "{} and {} confirmed spliced (the layout's leg splice proposal; {})",
                s.member,
                s.onto,
                s.assumed()
            )
        }))
        .collect();
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    if crate::leg_splices::confirm_places(&mut doc, t, &one, &spl, ask.leg_bolt.as_ref()).is_ok() {
        out.push(Base {
            prefix: "the layout's proposals here confirmed, then ",
            text: doc.to_string(),
            confirms: words,
        });
    }
    Ok(out)
}

/// The best candidate of `tried` strictly better than `before`, not
/// refused: the fewest clashes, then problems; of equal ones the first.
fn best_of(tried: &[Candidate], before: &Counts) -> Option<usize> {
    tried
        .iter()
        .enumerate()
        .filter(|(_, c)| c.refused.is_none() && c.counts.key() < before.key())
        .min_by_key(|(k, c)| (c.counts.key(), *k))
        .map(|(k, _)| k)
}

/// Why no way tried is better, in words.
fn why_none(tried: &[Candidate], before: &Counts) -> String {
    if tried.is_empty() {
        return "the layout changes nothing here and no fix is found for what is left".to_owned();
    }
    let mut parts: Vec<String> = Vec::new();
    for c in tried {
        let what = match &c.refused {
            Some(r) => format!("{}: refused — {r}", c.label),
            None => format!(
                "{}: {} clash(es), {} other problem(s) (now {}, {})",
                c.label, c.counts.clashes, c.counts.problems, before.clashes, before.problems
            ),
        };
        parts.push(what);
    }
    let hand: BTreeSet<&str> = tried
        .iter()
        .flat_map(|c| c.hand.iter().map(String::as_str))
        .collect();
    let mut out = format!("nothing tried leaves fewer: {}", parts.join("; "));
    if !hand.is_empty() {
        out.push_str(&format!(
            ". What blocks the layout: {}",
            hand.into_iter().collect::<Vec<_>>().join("; ")
        ));
    }
    out
}

/// The structure file `text` read.
fn parse(text: &str) -> Result<StructureType, String> {
    StructureType::parse(std::path::Path::new("structure.ohls"), text).map_err(|e| e.message)
}

/// What is left at `joint` and around it (`focus`) in `text`, and those
/// joints in detail (from the file cut down to what they read where that
/// is quicker).
fn measure(
    text: &str,
    ask: &SolveAsk<'_>,
    focus: &BTreeSet<String>,
    joint: &str,
) -> Result<(Counts, JointDetails), String> {
    let doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let keep = crate::reduce::members_for(&doc, focus);
    let cut = if crate::reduce::worth(&doc, &keep) {
        crate::reduce::cut_down(&doc, &keep).to_string()
    } else {
        text.to_owned()
    };
    let d = crate::joint_details_at(&cut, ask.library, ask.bolts, Some(focus))?;
    let of = |j: &JointDetail| {
        j.arrivals
            .iter()
            .flat_map(|a| &a.problems)
            .filter(|(p, _)| !matches!(p, Problem::Clash { .. }))
            .count()
    };
    let mut c = Counts::default();
    for j in &d.joints {
        if j.id == joint {
            c.clashes = j.clashes.len();
            c.problems = of(j);
        } else if focus.contains(&j.id) {
            c.around += j.clashes.len() + of(j);
        }
    }
    Ok((c, d))
}

/// Why the change from `t0` to `t1` is not taken at joint `j0` (as it was;
/// `j1` after): a leg's or a member running on's steel pulled back, a leg
/// turned (unless among `turned_ok`), a section changed.
fn refusal(
    t0: &StructureType,
    t1: &StructureType,
    j0: &JointDetail,
    j1: Option<&JointDetail>,
    turned_ok: &BTreeSet<String>,
) -> Option<String> {
    for a in &j0.arrivals {
        let (Some(m0), Some(m1)) = (
            t0.members.iter().find(|m| m.id == a.member),
            t1.members.iter().find(|m| m.id == a.member),
        ) else {
            continue;
        };
        if m0.section != m1.section {
            return Some(format!("it changes {}'s section", a.member));
        }
        if m0.nodes != m1.nodes {
            return Some(format!("it moves {}'s ends", a.member));
        }
        let leg = a.class.as_deref() == Some("leg");
        let r0 = ohl_structure_edit::edits::member_roll(m0);
        let r1 = ohl_structure_edit::edits::member_roll(m1);
        if leg && (r0 - r1).abs() > 1e-6 && !turned_ok.contains(&a.member) {
            return Some(format!("it turns the leg {}", a.member));
        }
        if crate::joint_fixes::runs_on(j0, a)
            && let Some(b) = j1.and_then(|j| j.arrivals.iter().find(|b| b.member == a.member))
        {
            let c0 = a.cut.value.unwrap_or(0.0);
            let c1 = b.cut.value.unwrap_or(0.0);
            if c1 > c0 + 1e-4 {
                return Some(format!(
                    "it pulls back {} ({}), which runs on through the joint",
                    a.member,
                    if leg { "a leg" } else { "a member in line" }
                ));
            }
        }
    }
    None
}

/// What changes at the joint from `j0` (in `t0`) to `j1` (in `t1`), in
/// words.
fn diff(
    t0: &StructureType,
    t1: &StructureType,
    j0: &JointDetail,
    j1: Option<&JointDetail>,
) -> Vec<Effect> {
    let mut out = Vec::new();
    let Some(j1) = j1 else { return out };
    let mm = |v: f64| format!("{:.0} mm", v * 1000.0);
    for b in &j1.arrivals {
        let Some(a) = j0.arrivals.iter().find(|a| a.member == b.member) else {
            continue;
        };
        let m = &b.member;
        if let (Some(m0), Some(m1)) = (
            t0.members.iter().find(|x| &x.id == m),
            t1.members.iter().find(|x| &x.id == m),
        ) {
            let r0 = ohl_structure_edit::edits::member_roll(m0);
            let r1 = ohl_structure_edit::edits::member_roll(m1);
            let mut d = (r1 - r0).to_degrees() % 360.0;
            if d > 180.0 {
                d -= 360.0;
            } else if d <= -180.0 {
                d += 360.0;
            }
            if d.abs() >= 0.005 {
                out.push(Effect {
                    kind: EffectKind::Turned,
                    words: format!(
                        "{m} turned by {d:+.1}° about its axis (its other end and its axes for \
                         the analysis follow)"
                    ),
                });
            }
        }
        if a.onto.value != b.onto.value {
            out.push(Effect {
                kind: EffectKind::Onto,
                words: format!(
                    "{m} onto {} (was {})",
                    b.onto.value.as_deref().unwrap_or("nothing"),
                    a.onto.value.as_deref().unwrap_or("nothing")
                ),
            });
        } else if a.sits.value != b.sits.value
            && let Some(f) = b.sits.value
        {
            out.push(Effect {
                kind: EffectKind::Onto,
                words: format!(
                    "{m} on the {} face of {}",
                    f.as_str(),
                    b.onto.value.as_deref().unwrap_or("?")
                ),
            });
        }
        let (c0, c1) = (a.cut.value.unwrap_or(0.0), b.cut.value.unwrap_or(0.0));
        if (c0 - c1).abs() >= 5e-4 && b.continuous.is_none() {
            out.push(Effect {
                kind: EffectKind::Cut,
                words: format!("{m}'s steel stops at {} (was {})", mm(c1), mm(c0)),
            });
        }
        if (a.shift.0 - b.shift.0).abs() >= 5e-5 {
            out.push(Effect {
                kind: EffectKind::Moved,
                words: format!(
                    "{m} moved sideways {:+.1} mm (was {:+.1} mm): an eccentricity the analysis \
                     takes",
                    b.shift.0 * 1000.0,
                    a.shift.0 * 1000.0
                ),
            });
        }
        if a.continuous.is_none() && b.continuous.is_some() {
            out.push(Effect {
                kind: EffectKind::Onto,
                words: format!(
                    "{m} runs on into {} as one part",
                    b.continuous.as_deref().unwrap_or("?")
                ),
            });
        }
    }
    for p in &j1.plates {
        if !j0.plates.iter().any(|q| q.id == p.id) {
            out.push(Effect {
                kind: EffectKind::PlateAdded,
                words: format!(
                    "plate {} added, {} on {} ({})",
                    p.id,
                    mm(p.thickness),
                    p.leg.as_deref().unwrap_or("the joint"),
                    if p.members.is_empty() {
                        "nothing bolted onto it: the flat plate a plate in its member's own plane \
                         is bent from or welded to"
                            .to_owned()
                    } else {
                        p.members.join(", ")
                    }
                ),
            });
        }
    }
    for p in &j0.plates {
        if !j1.plates.iter().any(|q| q.id == p.id) {
            out.push(Effect {
                kind: EffectKind::PlateRemoved,
                words: format!("plate {} taken away", p.id),
            });
        }
    }
    out
}

/// The fixes of the problems left at `joint` in `text` (with `counts`),
/// one at a time, each the one leaving fewest, kept where the joint is
/// better after it and the joints around no worse; the text after and the
/// fixes made (in words).
fn fix_rounds(
    text: &str,
    counts: Counts,
    ask: &SolveAsk<'_>,
    focus: &BTreeSet<String>,
    joint: &str,
    t0: &StructureType,
    j0: &JointDetail,
) -> Result<(String, Vec<String>), String> {
    let mut now = text.to_owned();
    let mut c_now = counts;
    let mut made: Vec<String> = Vec::new();
    for _ in 0..FIX_ROUNDS {
        let (_, d) = measure(&now, ask, focus, joint)?;
        let t = parse(&now)?;
        let Some(j) = d.joint(joint) else { break };
        let mut fixes: Vec<crate::joint_fixes::Fix> = Vec::new();
        for a in &j.arrivals {
            for (p, _) in &a.problems {
                for f in crate::joint_fixes::candidates(&t, j, a, p) {
                    if !fixes.contains(&f) {
                        fixes.push(f);
                    }
                }
            }
        }
        let mut best: Option<(Counts, String, String)> = None;
        for f in fixes.into_iter().take(FIX_TRIES) {
            let Some(after) = crate::joint_fixes::applied(&now, &f) else {
                continue;
            };
            let Ok((c, d1)) = measure(&after, ask, focus, joint) else {
                continue;
            };
            if c.key() >= c_now.key() || c.around > c_now.around {
                continue;
            }
            let Ok(t1) = parse(&after) else { continue };
            if refusal(t0, &t1, j0, d1.joint(joint), &BTreeSet::new()).is_some() {
                continue;
            }
            if best.as_ref().is_none_or(|b| c.key() < b.0.key()) {
                best = Some((c, after, f.label.clone()));
            }
        }
        match best {
            Some((c, after, label)) => {
                c_now = c;
                now = after;
                made.push(label);
                if c.clashes + c.problems == 0 {
                    break;
                }
            }
            None => break,
        }
    }
    Ok((now, made))
}

/// The leg pieces (and, O44 L13, the chord pieces running on) at the joint
/// whose joints say they are turned ([`crate::leg_roll`], L11), and (O44
/// L18) the leg's and chord's runs at the joint that their joints' planes
/// turn ([`crate::plane_rolls`]) — side by side, each with the solution
/// worked out again with its turn. The turn is never judged on one joint
/// (the owner, 2026-11-22: "when turning a member both sides should be
/// reverified"): every joint the turned members reach is laid out with it
/// and checked ([`TurnReach`]); a solution leaving more clashes and problems
/// there is refused.
fn leg_turns(
    text: &str,
    ask: &SolveAsk<'_>,
    t0: &StructureType,
    j0: &JointDetail,
    bases: &[Base],
    focus: &BTreeSet<String>,
) -> Result<Vec<LegTurn>, String> {
    // (O44 L13) And the chord pieces running on through it (an arm's chord,
    // a body's member running on: `joint_classes::chord_pieces`), their
    // turn read as a leg's — proposed as the leg turns are (the owner's
    // answer to L12's question 4: off in a run, offered with a tick).
    let chord_at: Vec<&str> = crate::joint_classes::chord_pieces(j0)
        .into_iter()
        .filter_map(|i| j0.arrivals.get(i))
        .map(|a| a.member.as_str())
        .collect();
    let legs: Vec<&str> = j0
        .arrivals
        .iter()
        .filter(|a| a.class.as_deref() == Some("leg"))
        .map(|a| a.member.as_str())
        .chain(chord_at.iter().copied())
        .collect();
    if legs.is_empty() {
        return Ok(Vec::new());
    }
    // The legs' joints (both ends).
    let joints: BTreeSet<String> = t0
        .members
        .iter()
        .filter(|m| legs.contains(&m.id.as_str()))
        .flat_map(|m| m.nodes.iter().cloned())
        .collect();
    let doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let keep = crate::reduce::members_for(&doc, &joints);
    let cut = if crate::reduce::worth(&doc, &keep) {
        crate::reduce::cut_down(&doc, &keep).to_string()
    } else {
        text.to_owned()
    };
    let d = crate::joint_details_at(&cut, ask.library, ask.bolts, Some(&joints))?;
    let mut found = crate::leg_rolls(&d);
    if !chord_at.is_empty() {
        found.extend(crate::leg_roll::piece_rolls(
            &d,
            &crate::joint_classes::chord_pieces,
        ));
    }
    let rolls = crate::leg_roll::with_faces(found, text, ask.library);
    // (The proposals, each: its source, the members turned alike, the turn,
    // its words, the leg-rotation check's or the planes' own.)
    let mut props: Vec<TurnProposal> = Vec::new();
    let mut done: BTreeSet<String> = BTreeSet::new();
    for r in rolls
        .into_iter()
        .filter(|r| r.flagged() && legs.contains(&r.member.as_str()))
    {
        if done.contains(&r.member) {
            continue;
        }
        let Some(turn) = r.turn else { continue };
        let mut members: Vec<String> = vec![r.member.clone()];
        members.extend(r.partners.iter().cloned());
        members.sort();
        members.dedup();
        done.extend(members.iter().cloned());
        let words = r.words();
        props.push((TurnSource::Joints, members, turn, words, Some(r), None));
    }
    // (O44 L18) The runs of the leg and chord pieces here as their joints'
    // planes turn them (on the whole tower: a run reaches past this joint's
    // members).
    let at = BTreeSet::from([j0.id.clone()]);
    let whole = crate::joint_details(text, ask.library, ask.bolts)?;
    for p in crate::plane_rolls::proposed_runs(&whole, Some(&at)) {
        if !p.members.iter().any(|m| legs.contains(&m.as_str())) {
            continue;
        }
        props.push((
            TurnSource::Planes,
            p.members.clone(),
            p.turn,
            p.words(),
            None,
            Some(p),
        ));
    }
    let mut out = Vec::new();
    for (source, members, turn, words, roll, plane) in props {
        let ok: BTreeSet<String> = members.iter().cloned().collect();
        // Every joint the members reach (both ends of each), and as they
        // are there.
        let reached: BTreeSet<String> = t0
            .members
            .iter()
            .filter(|m| ok.contains(&m.id))
            .flat_map(|m| m.nodes.iter().cloned())
            .chain(std::iter::once(j0.id.clone()))
            .collect();
        let wide: BTreeSet<String> = focus.union(&reached).cloned().collect();
        let reach_before = problems_at(text, ask, &reached)?;
        // On each starting point, the layout of every joint reached with
        // the turn (values given by hand kept, then replaced): the best.
        let mut best: Option<(Candidate, TurnReach)> = None;
        for b in bases {
            let mut doc: DocumentMut = b.text.parse().map_err(|e| format!("not TOML: {e}"))?;
            let tb = parse(&b.text)?;
            for m in &members {
                let now = tb
                    .members
                    .iter()
                    .find(|x| &x.id == m)
                    .map_or(0.0, ohl_structure_edit::edits::member_roll);
                let mut deg = (now.to_degrees() + turn) % 360.0;
                if deg > 180.0 {
                    deg -= 360.0;
                } else if deg <= -180.0 {
                    deg += 360.0;
                }
                deg = (deg * 100.0).round() / 100.0;
                ohl_structure_edit::edits::set_member_roll(&mut doc, m, deg.to_radians());
            }
            let turned = doc.to_string();
            for replace in [false, true] {
                let run = crate::lay_out(
                    &turned,
                    &RunAsk {
                        library: ask.library,
                        bolts: ask.bolts,
                        settings: ask.settings,
                        preset_fields: ask.preset_fields.clone(),
                        joints: Some(reached.clone()),
                        replace_hand: replace,
                    },
                )?;
                let (mut counts, d1) = measure(&run.text, ask, &wide, &j0.id)?;
                let outcome = run.joints.iter().find(|o| o.joint == j0.id);
                counts.left = outcome.map_or(0, |o| o.plan.hand_members.len());
                let after = d1
                    .joints
                    .iter()
                    .filter(|j| reached.contains(&j.id))
                    .map(problems_of)
                    .sum::<usize>();
                let reach = TurnReach {
                    joints: reached.iter().cloned().collect(),
                    before: reach_before,
                    after,
                    whole: run
                        .laid_out()
                        .filter(|o| o.hand.is_empty() && reached.contains(&o.joint))
                        .count(),
                    ends: run
                        .laid_out()
                        .filter(|o| reached.contains(&o.joint))
                        .map(|o| o.plan.ends.len())
                        .sum(),
                };
                let t1 = parse(&run.text)?;
                let refused = refusal(t0, &t1, j0, d1.joint(&j0.id), &ok).or_else(|| {
                    (after > reach_before).then(|| {
                        format!(
                            "more clashes and problems at the joints the turned members reach \
                             ({reach_before} to {after})"
                        )
                    })
                });
                let mut effects: Vec<Effect> = b
                    .confirms
                    .iter()
                    .map(|w| Effect {
                        kind: EffectKind::Confirmed,
                        words: w.clone(),
                    })
                    .collect();
                effects.push(Effect {
                    kind: EffectKind::Turned,
                    words: format!(
                        "{} turned by {turn:+.2}° (proposed: {words}); every joint they reach \
                         laid out with it ({})",
                        members.join(", "),
                        reach.joints.len()
                    ),
                });
                effects.extend(diff(t0, &t1, j0, d1.joint(&j0.id)).into_iter().filter(|e| {
                    !(e.kind == EffectKind::Turned
                        && members
                            .iter()
                            .any(|m| e.words.starts_with(&format!("{m} "))))
                }));
                let hand = outcome.map(|o| o.hand.clone()).unwrap_or_default();
                effects.extend(hand.iter().map(|h| Effect {
                    kind: EffectKind::Left,
                    words: format!("left for a hand detail — {h}"),
                }));
                let c = Candidate {
                    label: format!(
                        "{} turned by {turn:+.2}°, {}the layout of every joint they reach{}",
                        members.join(", "),
                        b.prefix,
                        if replace {
                            ", values given by hand replaced"
                        } else {
                            ""
                        }
                    ),
                    text: run.text.clone(),
                    counts,
                    refused,
                    effects,
                    hand,
                };
                if c.refused.is_none()
                    && best.as_ref().is_none_or(|(x, r)| {
                        (c.counts.key(), reach.after) < (x.counts.key(), r.after)
                    })
                {
                    best = Some((c, reach));
                }
            }
        }
        let (with, reach) = match best {
            Some((c, r)) => (Some(c), Some(r)),
            None => (None, None),
        };
        out.push(LegTurn {
            source,
            turn,
            words,
            roll,
            plane,
            members,
            with,
            reach,
        });
    }
    Ok(out)
}

/// A turn proposed: its source, the members turned alike, the turn
/// (degrees), its words, the leg-rotation check's or the planes' own.
type TurnProposal = (
    TurnSource,
    Vec<String>,
    f64,
    String,
    Option<crate::LegRoll>,
    Option<crate::plane_rolls::PlaneRoll>,
);

/// The clashes and other problems at joint `j`.
fn problems_of(j: &JointDetail) -> usize {
    j.clashes.len()
        + j.arrivals
            .iter()
            .flat_map(|a| &a.problems)
            .filter(|(p, _)| !matches!(p, Problem::Clash { .. }))
            .count()
}

/// The clashes and other problems at the joints `at` of `text`.
fn problems_at(text: &str, ask: &SolveAsk<'_>, at: &BTreeSet<String>) -> Result<usize, String> {
    let doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let keep = crate::reduce::members_for(&doc, at);
    let cut = if crate::reduce::worth(&doc, &keep) {
        crate::reduce::cut_down(&doc, &keep).to_string()
    } else {
        text.to_owned()
    };
    let d = crate::joint_details_at(&cut, ask.library, ask.bolts, Some(at))?;
    Ok(d.joints
        .iter()
        .filter(|j| at.contains(&j.id))
        .map(problems_of)
        .sum())
}
