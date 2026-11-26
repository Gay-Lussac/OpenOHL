// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Solve the clashes after the run** (O44 L30, the owner, 2026-11-26:
//! "Okay go ahead with the second run then"; L27's question 4: a pass after
//! a run that solves every joint left with clashes, applied only where
//! every joint around is no worse). Built, in active development, not
//! approved yet; [`crate::LayoutSettings::solve_left`], off by default.
//!
//! After the run (its weighed turns and passes included), every joint the
//! run left not whole — written with member ends left for a hand detail,
//! or not written though the layout had ends there — is given to *Solve
//! the clashes* ([`crate::solve::solve_joint_with`]), one joint after the
//! other in the run's order, each on the file as the joints before it
//! left it. What it tries there is what Joint details' window tries, less
//! what is the user's to decide: **no proposal confirmed** (pieces in line
//! one part, a leg splice: D15) and **no leg or chord turn proposed**; the
//! layout of the joint with values given by hand replaced only where the
//! run replaces them.
//!
//! A way tried is **taken** only where
//!
//! - the joint gets **better**: no more clashes, no more other problems and
//!   no more members left for a hand detail than the run left there, and
//!   fewer of one of them;
//! - **every joint around** (the other joints of the members meeting there)
//!   has no more clashes and other problems than before, joint by joint
//!   (Solve the clashes' own guard sums them);
//! - it changes **no value given by hand** (a connection's field the
//!   layout record does not keep as the layout's; a member's roll the
//!   designer gave, unless the run replaces values given by hand), and does
//!   not turn a member whose roll a weighing of the run set (L19's turns
//!   from the joints' planes, L25's half and quarter turns);
//! - and, as Solve the clashes refuses anyway, it pulls back no leg nor a
//!   member running on, turns no leg, changes no section and moves no end.
//!
//! Of the ways tried, the one with the fewest clashes, then other
//! problems, then members left that passes all of these is taken. Every
//! **turn** and **move** it makes (a member turned about its axis; an end
//! moved sideways, an eccentricity the analysis takes) is said as a
//! **marked** effect in the run's notes, with the rest of what it changes
//! ([`SolvedJoint::effects`]) — member size, work point and leg roll are
//! never changed. The values it writes are the layout's (kept in the
//! layout record: a later run takes them out first and solves again), the
//! members it turns too (their roll before kept, as the run's own turns).
//! Deterministic: the same file and settings give the same file, and a run
//! again on the result gives the same file (tested).

use std::collections::{BTreeMap, BTreeSet};

use ohl_structures::{LibraryItem, StructureType};
use toml_edit::DocumentMut;

use crate::joints::Problem;
use crate::layout_run::{LayoutRecord, LayoutRun, RunAsk};
use crate::solve::{Candidate, Counts, Effect, SolveAsk, SolveOptions};
use crate::weighed_turns::roll_text;

/// A joint the run left not whole, as the pass takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassJoint {
    /// The joint.
    pub joint: String,
    /// The members the run left for a hand detail there (written: those
    /// the plan left; not written: every member the layout had there).
    pub left: usize,
    /// Why (the run's words, one per member end).
    pub hand: Vec<String>,
}

/// The joints `run` left not whole, in its order: written with ends left
/// for a hand detail, or not written though the layout had ends there.
pub fn pass_joints(run: &LayoutRun) -> Vec<PassJoint> {
    run.joints
        .iter()
        .filter(|o| !(o.written && o.hand.is_empty()))
        .filter(|o| !(o.plan.ends.is_empty() && o.plan.hand_members.is_empty()))
        .map(|o| PassJoint {
            joint: o.joint.clone(),
            left: if o.written {
                o.plan.hand_members.len()
            } else {
                o.plan.ends.len() + o.plan.hand_members.len()
            },
            hand: o.hand.clone(),
        })
        .collect()
}

/// A joint the pass solved.
#[derive(Debug, Clone, PartialEq)]
pub struct SolvedJoint {
    /// The joint.
    pub joint: String,
    /// The way taken (Solve the clashes' words).
    pub label: String,
    /// What the run left there (clashes, other problems, members left).
    pub before: Counts,
    /// What is left after.
    pub after: Counts,
    /// What it changes; [`Effect::marked`]: a turn, a move (said marked).
    pub effects: Vec<Effect>,
    /// What is still left for a hand detail there, with why.
    pub hand: Vec<String>,
    /// Laid out whole now, as a run counts it: nothing left for a hand detail, no clash.
    pub whole: bool,
}

/// What the pass did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PassResult {
    /// The file after.
    pub text: String,
    /// The joints solved, in order.
    pub solved: Vec<SolvedJoint>,
    /// The joints tried and not solved, with why.
    pub not_solved: Vec<(String, String)>,
    /// How long it took, s.
    pub seconds: f64,
}

impl PassResult {
    /// The run's notes on the pass: one per joint solved (its marked
    /// effects said), and a summary.
    pub fn notes(&self) -> Vec<String> {
        let mut out = Vec::new();
        for s in &self.solved {
            let marked: Vec<&str> = s
                .effects
                .iter()
                .filter(|e| e.marked())
                .map(|e| e.words.as_str())
                .collect();
            let rest = s
                .effects
                .iter()
                .filter(|e| !e.marked() && e.kind != crate::solve::EffectKind::Left)
                .count();
            out.push(format!(
                "Solve the clashes after the run (L30) at {}: {}; clashes {} → {}, other \
                 problems {} → {}, members left for a hand detail {} → {}{}{}{}",
                s.joint,
                s.label,
                s.before.clashes,
                s.after.clashes,
                s.before.problems,
                s.after.problems,
                s.before.left,
                s.after.left,
                if s.whole { "; now laid out whole" } else { "" },
                if marked.is_empty() {
                    String::new()
                } else {
                    format!(
                        "; MARKED (it turns or moves steel: check it): {}",
                        marked.join("; ")
                    )
                },
                if rest == 0 {
                    String::new()
                } else {
                    format!("; {rest} other change(s) (Joint details lists them)")
                }
            ));
        }
        let whole = self.solved.iter().filter(|s| s.whole).count();
        let marked = self
            .solved
            .iter()
            .filter(|s| s.effects.iter().any(Effect::marked))
            .count();
        out.push(format!(
            "Solve the clashes after the run (L30): {} joint(s) left not whole tried, {} solved \
             ({whole} now whole, {marked} with a turn or move, marked), {} not; {:.0} s",
            self.solved.len() + self.not_solved.len(),
            self.solved.len(),
            self.not_solved.len(),
            self.seconds
        ));
        out
    }
}

/// The pass on the file `text` a run laid out, over the joints `left` it
/// left not whole (see the module), with the run's `ask`.
pub fn solve_left(text: &str, left: &[PassJoint], ask: &RunAsk<'_>) -> Result<PassResult, String> {
    let start = std::time::Instant::now();
    let mut now = text.to_owned();
    let mut out = PassResult::default();
    for pj in left {
        let sask = SolveAsk {
            library: ask.library,
            bolts: ask.bolts,
            settings: ask.settings,
            preset_fields: ask.preset_fields.clone(),
            leg_bolt: None,
            joint: pj.joint.clone(),
        };
        let opts = SolveOptions {
            proposals: false,
            leg_turns: false,
            replace_hand: ask.replace_hand,
            before_left: Some(pj.left),
        };
        let sol = match crate::solve::solve_joint_with(&now, &sask, opts) {
            Ok(s) => s,
            Err(e) => {
                out.not_solved
                    .push((pj.joint.clone(), format!("not solved: {e}")));
                continue;
            }
        };
        let before = sol.before;
        let key = |c: &Counts| (c.clashes, c.problems, c.left);
        let mut order: Vec<usize> = (0..sol.tried.len())
            .filter(|&k| {
                let c = &sol.tried[k];
                c.refused.is_none() && better(&c.counts, &before)
            })
            .collect();
        order.sort_by_key(|&k| (key(&sol.tried[k].counts), k));
        if order.is_empty() {
            out.not_solved.push((
                pj.joint.clone(),
                sol.why_none
                    .clone()
                    .unwrap_or_else(|| "no way tried is better on every count".to_owned()),
            ));
            continue;
        }
        let t0 = parse(&now)?;
        let rec0 = LayoutRecord::of(&t0);
        let focus: BTreeSet<String> = t0
            .members
            .iter()
            .filter(|m| m.nodes.contains(&pj.joint))
            .flat_map(|m| m.nodes.iter().cloned())
            .chain(std::iter::once(pj.joint.clone()))
            .collect();
        let around0 = per_joint(&now, ask, &focus)?;
        let f0 = fields_at(&now, &pj.joint)?;
        let mut why_not: Vec<String> = Vec::new();
        let mut taken = false;
        for k in order {
            let c = &sol.tried[k];
            let t1 = parse(&c.text)?;
            let f1 = fields_at(&c.text, &pj.joint)?;
            if let Some(w) = refusal((&t0, &rec0, &f0), (&t1, &f1), &pj.joint, ask.replace_hand) {
                why_not.push(format!("{}: {w}", c.label));
                continue;
            }
            let around1 = per_joint(&c.text, ask, &focus)?;
            if let Some(j) = focus
                .iter()
                .filter(|j| **j != pj.joint)
                .find(|j| around1.get(*j).unwrap_or(&0) > around0.get(*j).unwrap_or(&0))
            {
                why_not.push(format!(
                    "{}: more clashes and problems at joint {j} ({} to {})",
                    c.label,
                    around0.get(j).unwrap_or(&0),
                    around1.get(j).unwrap_or(&0)
                ));
                continue;
            }
            let hand = hand_after(c, pj);
            // (Whole as a run counts it: laid out, no end left, no clash.)
            let whole = hand.is_empty() && c.counts.clashes == 0 && c.counts.left == 0;
            now = recorded((&t0, &rec0, &f0), (&t1, &f1), c, &pj.joint, &hand)?;
            out.solved.push(SolvedJoint {
                joint: pj.joint.clone(),
                label: c.label.clone(),
                before,
                after: c.counts,
                effects: c.effects.clone(),
                hand,
                whole,
            });
            taken = true;
            break;
        }
        if !taken {
            out.not_solved.push((
                pj.joint.clone(),
                format!("not taken: {}", why_not.join("; ")),
            ));
        }
    }
    out.text = now;
    out.seconds = start.elapsed().as_secs_f64();
    Ok(out)
}

/// The pass after the run `run` (asked `ask`): its file, joints and notes
/// updated.
pub(crate) fn after_run(mut run: LayoutRun, ask: &RunAsk<'_>) -> Result<LayoutRun, String> {
    let left = pass_joints(&run);
    if left.is_empty() {
        return Ok(run);
    }
    let res = solve_left(&run.text, &left, ask)?;
    let t = parse(&res.text)?;
    let rec = LayoutRecord::of(&t);
    for s in &res.solved {
        if let Some(o) = run.joints.iter_mut().find(|o| o.joint == s.joint) {
            o.hand = s.hand.clone();
            o.written = o.written || rec.joints.contains(&s.joint);
            o.plates = rec
                .made
                .iter()
                .filter(|(_, (j, _))| *j == s.joint)
                .map(|(id, _)| id.clone())
                .collect();
        }
    }
    run.notes.extend(res.notes());
    run.seconds += res.seconds;
    run.text = res.text;
    run.solved = res.solved;
    Ok(run)
}

/// `after` is better than `before`: none of clashes, other problems and
/// members left more, one of them fewer.
fn better(after: &Counts, before: &Counts) -> bool {
    after.clashes <= before.clashes
        && after.problems <= before.problems
        && after.left <= before.left
        && (after.clashes, after.problems, after.left)
            != (before.clashes, before.problems, before.left)
}

/// What is still left for a hand detail at the joint after `c`: the
/// layout's words where it laid the joint out again; where only fixes were
/// made on the joint as the run left it, the run's words (`pj`), the ends
/// stay as they were.
fn hand_after(c: &Candidate, pj: &PassJoint) -> Vec<String> {
    if c.label.starts_with("the fixes of the joint's problems") {
        pj.hand.clone()
    } else {
        c.hand.clone()
    }
}

/// Why the change from `t0` (its record `rec0`) to `t1` at joint `joint`
/// is not taken by the pass: a value given by hand changed (a field of a
/// member end at the joint; a member's roll the designer gave, unless
/// `replace_hand`), or a member turned whose roll a weighing of the run
/// set.
fn refusal(
    (t0, rec0, doc0): (&StructureType, &LayoutRecord, &Fields),
    (t1, doc1): (&StructureType, &Fields),
    joint: &str,
    replace_hand: bool,
) -> Option<String> {
    for m0 in &t0.members {
        let Some(m1) = t1.members.iter().find(|x| x.id == m0.id) else {
            return Some(format!("{} is gone", m0.id));
        };
        if roll_text(m0.roll.as_ref()) == roll_text(m1.roll.as_ref()) {
            continue;
        }
        let m = &m0.id;
        if rec0.flange_turns.contains_key(m) || rec0.plane_turns.contains_key(m) {
            return Some(format!(
                "it turns {m}, whose roll a weighing of the run set (L19, L25)"
            ));
        }
        let program =
            m0.roll.is_none() || rec0.rolled.contains_key(m) || rec0.oriented.contains_key(m);
        if !program && !replace_hand {
            return Some(format!("it turns {m}, whose roll is given by hand"));
        }
    }
    for ((m, _), f0) in doc0 {
        let auto = rec0.ends.get(&(m.clone(), joint.to_owned()));
        let f1 = doc1.get(&(m.clone(), joint.to_owned()));
        for (k, v0) in f0 {
            let changed = f1.and_then(|f| f.get(k)) != Some(v0);
            let layouts = auto.is_some_and(|a| a.contains(k));
            let replaced = replace_hand && matches!(k.as_str(), "cut" | "shift" | "onto");
            if changed && !layouts && !replaced {
                return Some(format!("it changes {m}'s {k}, given by hand"));
            }
        }
    }
    None
}

/// The connection fields of a member end: (member, joint) → field → value
/// as written.
type Fields = BTreeMap<(String, String), BTreeMap<String, String>>;

/// The connection fields of every member end at `joint` of the file
/// `text`.
fn fields_at(text: &str, joint: &str) -> Result<Fields, String> {
    let tab: toml::Table = toml::from_str(text).map_err(|e| format!("not TOML: {e}"))?;
    let mut out = Fields::new();
    let Some(toml::Value::Array(a)) = tab.get("connection") else {
        return Ok(out);
    };
    for c in a.iter().filter_map(toml::Value::as_table) {
        let (Some(m), Some(j)) = (
            c.get("member").and_then(toml::Value::as_str),
            c.get("joint").and_then(toml::Value::as_str),
        ) else {
            continue;
        };
        if j != joint {
            continue;
        }
        let f: BTreeMap<String, String> = c
            .iter()
            .filter(|(k, _)| *k != "member" && *k != "joint")
            .map(|(k, x)| (k.clone(), x.to_string()))
            .collect();
        out.insert((m.to_owned(), j.to_owned()), f);
    }
    Ok(out)
}

/// The file after way `c` at `joint`, its layout record keeping what the
/// way wrote as the layout's: each field of a member end there it set
/// (not the layout's yet), each member it turned (its roll before; a
/// later run starts from it), and what is left for a hand detail there
/// (`hand`; none: no row).
fn recorded(
    (t0, rec0, f0): (&StructureType, &LayoutRecord, &Fields),
    (t1, f1s): (&StructureType, &Fields),
    c: &Candidate,
    joint: &str,
    hand: &[String],
) -> Result<String, String> {
    let mut rec = LayoutRecord::of(t1);
    for (key, f1) in f1s {
        let was = f0.get(key);
        for (k, v) in f1 {
            if was.and_then(|w| w.get(k)) != Some(v) {
                rec.ends.entry(key.clone()).or_default().insert(k.clone());
            }
        }
    }
    for m0 in &t0.members {
        let Some(m1) = t1.members.iter().find(|x| x.id == m0.id) else {
            continue;
        };
        let r0 = roll_text(m0.roll.as_ref());
        if r0 != roll_text(m1.roll.as_ref()) && !rec.rolled.contains_key(&m0.id) {
            rec.rolled.insert(
                m0.id.clone(),
                rec0.rolled.get(&m0.id).cloned().unwrap_or(r0),
            );
        }
    }
    if hand.is_empty() {
        rec.hand.remove(joint);
    } else {
        rec.hand.insert(joint.to_owned(), hand.join("; "));
    }
    let mut doc: DocumentMut = c.text.parse().map_err(|e| format!("not TOML: {e}"))?;
    rec.write(&mut doc);
    Ok(doc.to_string())
}

/// The clashes and other problems at each of the joints `at` of `text`.
fn per_joint(
    text: &str,
    ask: &RunAsk<'_>,
    at: &BTreeSet<String>,
) -> Result<BTreeMap<String, usize>, String> {
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
        .map(|j| {
            let p = j
                .arrivals
                .iter()
                .flat_map(|a| &a.problems)
                .filter(|(p, _)| !matches!(p, Problem::Clash { .. }))
                .count();
            (j.id.clone(), j.clashes.len() + p)
        })
        .collect())
}

/// The structure file `text` read.
fn parse(text: &str) -> Result<StructureType, String> {
    StructureType::parse(std::path::Path::new("structure.ohls"), text).map_err(|e| e.message)
}
