// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Turns weighed over every joint they reach** (the automatic joint
//! layout's L19; the owner, 2026-11-22: "when turning a member both sides
//! should be reverified"). Built, in active development, not approved yet.
//!
//! An angle has one roll along its whole length, and a leg's or chord's run
//! in line is turned as one ([`crate::plane_rolls`]): a turn that lays one
//! joint out better may cost another. Orient angles sets the turns from the
//! joints' planes on an imported tower without laying anything out, and
//! lists them in the layout record as not weighed yet
//! ([`crate::LayoutRecord::plane_turns`]). The next layout run weighs them:
//!
//! 1. every listed turn taken back (each member at the roll it had before)
//!    and the tower laid out — the base;
//! 2. stage by stage in the owner's order — the legs' and chords' runs,
//!    then other turns (none written by the program; a measurement may add
//!    the leg-rotation check's), then the face members' runs — the stage's
//!    turns all applied on top of those kept so far and the tower laid out
//!    again; each turn kept only where **every joint its members reach**
//!    (both ends of each piece) is laid out no worse than before the stage
//!    (no fewer member ends, nor as many on more plates) and one better
//!    (more ends, or as many on fewer plates) — the run's guard;
//! 3. the turns kept laid out again without the others, and weighed again
//!    there (a turn may have helped only beside one refused) until every
//!    turn kept still helps ([`WEIGH_ROUNDS`] at most; none kept at that
//!    stage where they never agree);
//! 4. a turn not kept: its members' rolls back to what they were, the turn
//!    said in the run's notes and offered by Lay out joints as a proposal.
//!
//! The rows go with the run (the file written lists none): a run again
//! starts from the rolls kept and lays out the same. Each layout is the
//! whole run ([`crate::lay_out`] without the weighing): the weighing costs
//! the base and two or more runs more (measured and said in the roadmap).
//! Deterministic: the same file and settings give the same result.

use std::collections::{BTreeMap, BTreeSet};

use ohl_structures::{LibraryItem, StructureType};
use toml_edit::DocumentMut;

use crate::layout_run::{LayoutRecord, LayoutRun, RunAsk, lay_out_plain, put_roll};

/// The most layouts a stage's turns are weighed again with only those
/// still kept (the program's bound): none kept at that stage when they do
/// not agree by then.
pub const WEIGH_ROUNDS: usize = 4;

/// One member's roll before and with a turn, as written (empty: none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRoll {
    /// The member.
    pub member: String,
    /// Its roll before the turn.
    pub before: String,
    /// Its roll with the turn.
    pub with: String,
}

/// A turn to weigh: a run's members turned as one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnCandidate {
    /// The run (its first member).
    pub run: String,
    /// Its kind (`leg`, `chord`, `bracing`, or another word for a turn
    /// not from the joints' planes).
    pub kind: String,
    /// Its stage ([`stage_of`]): weighed after the turns of a lower stage.
    pub stage: u8,
    /// Its members' rolls before and with it.
    pub members: Vec<TurnRoll>,
}

impl TurnCandidate {
    /// The turn, degrees (the first member's roll with it less before).
    pub fn turn(&self) -> f64 {
        self.members.first().map_or(0.0, |r| {
            let mut d = deg(&r.with) - deg(&r.before);
            while d > 180.0 {
                d -= 360.0;
            }
            while d <= -180.0 {
                d += 360.0;
            }
            (d * 100.0).round() / 100.0
        })
    }
}

/// The stage of a turn of kind `kind`: the legs' and chords' runs first
/// (0), then any other turn (1), then the face members' (2) — the owner's
/// order.
pub fn stage_of(kind: &str) -> u8 {
    match kind {
        "leg" | "chord" => 0,
        "bracing" | "face" => 2,
        _ => 1,
    }
}

/// A turn as the run weighed it.
#[derive(Debug, Clone, PartialEq)]
pub struct WeighedTurn {
    /// The run (its first member).
    pub run: String,
    /// Its kind.
    pub kind: String,
    /// Its members.
    pub members: Vec<String>,
    /// The turn, degrees.
    pub turn: f64,
    /// Kept (written); else its members' rolls taken back.
    pub kept: bool,
    /// The joints its members reach.
    pub joints: Vec<String>,
    /// Its joints' member ends laid out and plates before its stage.
    pub before: (usize, usize),
    /// The same with it, as last laid out with it.
    pub after: (usize, usize),
    /// Its joints laid out worse with it (fewer ends, or as many on more
    /// plates), as last laid out with it.
    pub worse: Vec<String>,
}

impl WeighedTurn {
    /// In words.
    pub fn words(&self) -> String {
        let (e0, p0) = self.before;
        let (e1, p1) = self.after;
        if is_flange(&self.kind) {
            let what = format!(
                "{}: its outstanding leg into the flange, {} ({:+.0}°)",
                self.members.join(", "),
                self.kind,
                self.turn
            );
            return if self.kept {
                format!(
                    "{what}: kept — its {} joint(s) lay out {e1} member end(s) on {p1} plate(s) \
                     (before {e0} on {p0}), none worse",
                    self.joints.len()
                )
            } else if self.turn != 0.0 {
                format!(
                    "{what}: not applied (offered, or the turns kept together did not all \
                     still help); its {} joint(s) {e0} end(s) on {p0} plate(s) as it is",
                    self.joints.len()
                )
            } else {
                format!(
                    "{what}: neither turn helps over its {} joint(s) ({e0} end(s) on {p0} \
                     plate(s)); left",
                    self.joints.len()
                )
            };
        }
        let from = if stage_of(&self.kind) == 1 {
            "as its joints say"
        } else {
            "from its joints' planes"
        };
        let what = format!(
            "{} run {} turned by {:+.2}° ({from})",
            self.kind,
            self.members.join(", "),
            self.turn
        );
        if self.kept {
            format!(
                "{what}: kept — its {} joint(s) lay out {e1} member end(s) on {p1} plate(s) \
                 (before {e0} on {p0}), none worse",
                self.joints.len()
            )
        } else if self.worse.is_empty() {
            format!(
                "{what}: taken back (its {} joint(s) laid out no better with it: {e1} end(s) on \
                 {p1} plate(s), before {e0} on {p0}); offered in Lay out joints",
                self.joints.len()
            )
        } else {
            format!(
                "{what}: taken back (joint(s) {} laid out worse with it; its {} joint(s) {e1} \
                 end(s) on {p1} plate(s), before {e0} on {p0}); offered in Lay out joints",
                list(&self.worse),
                self.joints.len()
            )
        }
    }
}

/// The turns of structure `t` its layout record lists as not weighed yet
/// ([`LayoutRecord::plane_turns`]), by run; a run left out where a member
/// is gone or its roll is no longer the one written with the turn (changed
/// since: the user's), or where it turns nothing. Runs whose pieces are
/// written one part (`continuous`) weighed as one; a run one part with a
/// piece not listed, or with another roll before, left as written (taking
/// it back would part what is one part).
pub fn candidates_of(t: &StructureType) -> Vec<TurnCandidate> {
    let rec = LayoutRecord::of(t);
    let mut by_run: BTreeMap<&str, TurnCandidate> = BTreeMap::new();
    let mut bad: BTreeSet<&str> = BTreeSet::new();
    for (m, row) in &rec.plane_turns {
        let now = t
            .members
            .iter()
            .find(|x| &x.id == m)
            .map(|x| roll_text(x.roll.as_ref()));
        if now.as_deref() != Some(row.with.as_str()) {
            bad.insert(row.run.as_str());
            continue;
        }
        by_run
            .entry(row.run.as_str())
            .or_insert_with(|| TurnCandidate {
                run: row.run.clone(),
                kind: row.kind.clone(),
                stage: stage_of(&row.kind),
                members: Vec::new(),
            })
            .members
            .push(TurnRoll {
                member: m.clone(),
                before: row.before.clone(),
                with: row.with.clone(),
            });
    }
    // The members in the file's order (the run's first member first).
    let order: BTreeMap<&str, usize> = t
        .members
        .iter()
        .enumerate()
        .map(|(i, m)| (m.id.as_str(), i))
        .collect();
    let mut runs: Vec<TurnCandidate> = by_run
        .into_iter()
        .filter(|(k, _)| !bad.contains(k))
        .map(|(_, c)| c)
        .collect();
    // Pieces written one part (`continuous`) keep one roll: the runs they
    // join weighed as one; where a piece one part with a run's is not
    // listed, or the rolls before differ, the turn is left as written
    // (taking it back would part what is one part).
    let mut parent: Vec<usize> = (0..runs.len()).collect();
    fn root(p: &mut [usize], mut k: usize) -> usize {
        while p[k] != k {
            p[k] = p[p[k]];
            k = p[k];
        }
        k
    }
    let of: BTreeMap<String, usize> = runs
        .iter()
        .enumerate()
        .flat_map(|(i, c)| c.members.iter().map(move |r| (r.member.clone(), i)))
        .collect();
    let mut broken: BTreeSet<usize> = BTreeSet::new();
    let mut pairs: Vec<(String, String)> = Vec::new();
    for c in &t.connections {
        if c.kind() != ohl_structures::connections::ConnectionKind::Continuous {
            continue;
        }
        let Some(onto) = &c.onto else { continue };
        match (of.get(&c.member), of.get(onto)) {
            (Some(&a), Some(&b)) => {
                let (x, y) = (root(&mut parent, a), root(&mut parent, b));
                if x != y {
                    parent[x.max(y)] = x.min(y);
                }
                pairs.push((c.member.clone(), onto.clone()));
            }
            (Some(&a), None) | (None, Some(&a)) => {
                broken.insert(a);
            }
            (None, None) => {}
        }
    }
    let before: BTreeMap<&str, &str> = runs
        .iter()
        .flat_map(|c| {
            c.members
                .iter()
                .map(|r| (r.member.as_str(), r.before.as_str()))
        })
        .collect();
    for (a, b) in &pairs {
        if before.get(a.as_str()) != before.get(b.as_str()) {
            broken.insert(of[a]);
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..runs.len() {
        let r = root(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    let mut out = Vec::new();
    for (_, ks) in groups {
        if ks
            .iter()
            .any(|k| broken.contains(&root(&mut parent, *k)) || broken.contains(k))
        {
            continue;
        }
        let mut c = runs[ks[0]].clone();
        for &k in &ks[1..] {
            c.stage = c.stage.min(runs[k].stage);
            let mut more = std::mem::take(&mut runs[k].members);
            c.members.append(&mut more);
        }
        if c.members.iter().all(|r| r.before == r.with) {
            continue;
        }
        c.members
            .sort_by_key(|r| order.get(r.member.as_str()).copied().unwrap_or(usize::MAX));
        c.run = c.members[0].member.clone();
        out.push(c);
    }
    out.sort_by(|a, b| a.run.cmp(&b.run));
    out
}

/// The joints candidate `c` reaches in `t` (its members' both ends).
pub fn reach_of(t: &StructureType, c: &TurnCandidate) -> BTreeSet<String> {
    let set: BTreeSet<&str> = c.members.iter().map(|r| r.member.as_str()).collect();
    t.members
        .iter()
        .filter(|m| set.contains(m.id.as_str()))
        .flat_map(|m| m.nodes.iter().cloned())
        .collect()
}

/// The turns of `t` to weigh in a run asking for the joints `joints`
/// (none: every joint): those whose every joint is asked ([`candidates_of`]
/// and [`reach_of`]) — a turn is weighed only by a run laying out every
/// joint it reaches; the others stay listed for a run that does.
pub fn candidates_for(t: &StructureType, joints: Option<&BTreeSet<String>>) -> Vec<TurnCandidate> {
    candidates_of(t)
        .into_iter()
        .filter(|c| joints.is_none_or(|j| reach_of(t, c).is_subset(j)))
        .collect()
}

/// The layout run on `text` (the file with every turn of `cands` applied)
/// with each turn weighed over every joint it reaches (see the module); the
/// joints asked must hold every joint the turns reach ([`candidates_for`]).
/// Then (O44 L25) the half and quarter turns for an outstanding leg
/// pointing into the flange weighed on the file so settled
/// ([`crate::flange_turns`]).
pub fn weigh_turns(
    text: &str,
    cands: &[TurnCandidate],
    ask: &RunAsk<'_>,
) -> Result<LayoutRun, String> {
    let (r, settled, extra) = weigh(text, cands, ask)?;
    crate::flange_turns::weigh_flange_turns(&settled, r, &extra, ask).map(|x| x.0)
}

/// **Settle the turns** of `text` the layout record lists as not weighed
/// yet, on the whole tower: the file before any layout with each turn kept
/// only where the whole layout run (every joint, `ask`'s settings) lays
/// every joint it reaches out better, the others' rolls back to before,
/// their rows gone — and that run. None: nothing to weigh. For a window
/// that lays out the file several ways (the proposals' effects): weighed
/// once, the rest laid out from the settled file. O44 L25: then the half
/// and quarter turns for an outstanding leg pointing into the flange
/// weighed on the whole tower too ([`crate::flange_turns`]; their rows
/// written, so that a run of the settled file weighs them no more).
pub fn settle_turns(text: &str, ask: &RunAsk<'_>) -> Result<Option<(String, LayoutRun)>, String> {
    let whole = RunAsk {
        joints: None,
        ..ask.clone()
    };
    let (text1, r0, extra) = if ask.settings.weigh_turns && text.contains("plane_turns") {
        let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
            .map_err(|e| e.message)?;
        let cands = candidates_of(&t);
        if cands.is_empty() {
            (text.to_owned(), None, Vec::new())
        } else {
            let (run, settled, extra) = weigh(text, &cands, &whole)?;
            (settled, Some(run), extra)
        }
    } else {
        (text.to_owned(), None, Vec::new())
    };
    let flange = ask.settings.flange_half || ask.settings.flange_quarter;
    match (r0, flange) {
        (None, false) => Ok(None),
        (Some(r), false) => Ok(Some((text1, r))),
        (r0, true) => {
            let weighed_before = r0.is_some();
            let r0 = match r0 {
                Some(r) => r,
                None => lay_out_plain(&text1, &whole)?,
            };
            let (run, settled) =
                crate::flange_turns::weigh_flange_turns(&text1, r0, &extra, &whole)?;
            if !weighed_before && settled == text1 {
                return Ok(None);
            }
            Ok(Some((settled, run)))
        }
    }
}

/// [`weigh_turns`] without L25's turns, the file settled (before the
/// layout: the turns kept, the others taken back, their rows gone) and the
/// notes the weighing added to the run's.
fn weigh(
    text: &str,
    cands: &[TurnCandidate],
    ask: &RunAsk<'_>,
) -> Result<(LayoutRun, String, Vec<String>), String> {
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let doc0: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    // The turns in the order they are weighed (by stage, then as given).
    let mut order: Vec<usize> = (0..cands.len()).collect();
    order.sort_by_key(|&k| (cands[k].stage, k));
    // The base: every member at the roll before its first turn, its row
    // gone (the file written lists none of the turns weighed).
    let mut base = doc0.clone();
    let mut first: BTreeSet<&str> = BTreeSet::new();
    for &k in &order {
        for r in &cands[k].members {
            if first.insert(r.member.as_str()) {
                put_roll(&mut base, &r.member, &r.before);
            }
        }
    }
    let mut rec = LayoutRecord::of(&t);
    rec.plane_turns.retain(|m, _| !first.contains(m.as_str()));
    rec.write(&mut base);
    let text_with = |set: &BTreeSet<usize>| -> String {
        let mut d = base.clone();
        for &k in order.iter().filter(|k| set.contains(k)) {
            for r in &cands[k].members {
                put_roll(&mut d, &r.member, &r.with);
            }
        }
        d.to_string()
    };
    // The joints each turn reaches (its members' both ends).
    let reach: Vec<Vec<String>> = cands
        .iter()
        .map(|c| reach_of(&t, c).into_iter().collect())
        .collect();
    let wide = ask;
    let at = |r: &LayoutRun, k: usize| -> Vec<Counts> {
        let by = counts_of(r);
        reach[k]
            .iter()
            .map(|j| by.get(j.as_str()).copied().unwrap_or_default())
            .collect()
    };
    let sum = |v: &[Counts]| {
        v.iter()
            .fold((0, 0), |(e, p), c| (e + c.ends, p + c.plates))
    };
    let worse_at = |b: &[Counts], a: &[Counts], k: usize| -> Vec<String> {
        b.iter()
            .zip(a)
            .zip(&reach[k])
            .filter(|((b, a), _)| a.worse_than(b))
            .map(|(_, j)| j.clone())
            .collect()
    };
    // The members meeting at each joint (a joint laid out worse anywhere:
    // the turns reaching a member there the layout turned otherwise).
    let mut at_joint: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for m in &t.members {
        for n in &m.nodes {
            at_joint.entry(n.as_str()).or_default().push(m.id.as_str());
        }
    }
    let nodes_of: BTreeMap<&str, &[String; 2]> = t
        .members
        .iter()
        .map(|m| (m.id.as_str(), &m.nodes))
        .collect();
    let mut prev = lay_out_plain(&text_with(&BTreeSet::new()), wide)?;
    let mut seconds = prev.seconds;
    let mut layouts = 1usize;
    let mut unexplained = 0usize;
    let mut kept: BTreeSet<usize> = BTreeSet::new();
    let mut weighed: BTreeMap<usize, WeighedTurn> = BTreeMap::new();
    let stages: BTreeSet<u8> = cands.iter().map(|c| c.stage).collect();
    for stage in stages {
        let all: BTreeSet<usize> = order
            .iter()
            .copied()
            .filter(|&k| cands[k].stage == stage)
            .collect();
        let with =
            |set: &BTreeSet<usize>| -> BTreeSet<usize> { kept.union(set).copied().collect() };
        let mut tried = all.clone();
        let mut r = lay_out_plain(&text_with(&with(&tried)), wide)?;
        seconds += r.seconds;
        layouts += 1;
        let mut confirmed = false;
        let mut stage_left = 0;
        for round in 0..=WEIGH_ROUNDS {
            // Each turn tried: as last laid out with it.
            for &k in &tried {
                let (b, a) = (at(&prev, k), at(&r, k));
                let c = &cands[k];
                weighed.insert(
                    k,
                    WeighedTurn {
                        run: c.run.clone(),
                        kind: c.kind.clone(),
                        members: c.members.iter().map(|x| x.member.clone()).collect(),
                        turn: c.turn(),
                        kept: false,
                        joints: reach[k].clone(),
                        before: sum(&b),
                        after: sum(&a),
                        worse: worse_at(&b, &a, k),
                    },
                );
            }
            // Each turn kept where every joint it reaches lays out better
            // (the run's guard) — and where no joint elsewhere is laid out
            // worse at a member the layout turned otherwise beside it (a
            // bracing's one roll turned for its end at the turned member:
            // the owner's "both sides reverified" past the turned members).
            let (blamed, left) = knock_on(&prev, &r, &tried, &reach, cands, &at_joint, &nodes_of)?;
            stage_left = left;
            let keep: BTreeSet<usize> = tried
                .iter()
                .copied()
                .filter(|&k| !blamed.contains(&k) && helps(&at(&prev, k), &at(&r, k)))
                .collect();
            if keep == tried {
                confirmed = true;
                break;
            }
            if keep.is_empty() || round == WEIGH_ROUNDS {
                break;
            }
            tried = keep;
            r = lay_out_plain(&text_with(&with(&tried)), wide)?;
            seconds += r.seconds;
            layouts += 1;
        }
        if confirmed && !tried.is_empty() {
            for &k in &tried {
                if let Some(w) = weighed.get_mut(&k) {
                    w.kept = true;
                }
            }
            kept.extend(tried);
            prev = r;
            unexplained += stage_left;
        }
    }
    let mut out = prev;
    out.seconds = seconds;
    let list: Vec<WeighedTurn> = order.iter().filter_map(|k| weighed.remove(k)).collect();
    let mut extra = notes_of(&list);
    extra.push(format!(
        "the weighing laid the tower out {layouts} times ({seconds:.1} s, measured)"
    ));
    if unexplained > 0 {
        extra.push(format!(
            "{unexplained} joint(s) laid out worse after the last turns kept where no turn \
             reaches nor a member turned otherwise meets them (the layout's order)"
        ));
    }
    out.notes.extend(extra.iter().cloned());
    out.weighed = list;
    Ok((out, text_with(&kept), extra))
}

/// The joints laid out worse in `r` than in `prev` (fewer member ends, or
/// as many on more plates; anywhere in the tower), their turns of `tried`
/// to blame: those reaching a joint of a member meeting there whose roll the
/// layout wrote otherwise in `r` (or that is the turn's own). And how many
/// such joints no turn reaches and no member turned otherwise meets.
pub(crate) fn knock_on(
    prev: &LayoutRun,
    r: &LayoutRun,
    tried: &BTreeSet<usize>,
    reach: &[Vec<String>],
    cands: &[TurnCandidate],
    at_joint: &BTreeMap<&str, Vec<&str>>,
    nodes_of: &BTreeMap<&str, &[String; 2]>,
) -> Result<(BTreeSet<usize>, usize), String> {
    let (b, a) = (counts_of(prev), counts_of(r));
    let worse: Vec<&str> = a
        .iter()
        .filter(|(j, x)| x.worse_than(&b.get(*j).copied().unwrap_or_default()))
        .map(|(j, _)| *j)
        .collect();
    let mut blamed = BTreeSet::new();
    if worse.is_empty() {
        return Ok((blamed, 0));
    }
    let (rp, ra) = (rolls_in(&prev.text)?, rolls_in(&r.text)?);
    let mut left = 0;
    for j in worse {
        let reached: Vec<usize> = tried
            .iter()
            .copied()
            .filter(|&k| reach[k].iter().any(|x| x == j))
            .collect();
        let turned: Vec<&str> = at_joint
            .get(j)
            .into_iter()
            .flatten()
            .copied()
            .filter(|m| rp.get(*m) != ra.get(*m))
            .collect();
        let mut any = !reached.is_empty();
        for m in turned {
            let ends = nodes_of.get(m).copied();
            for &k in tried {
                let own = cands[k].members.iter().any(|x| x.member == m);
                let meets = ends.is_some_and(|e| reach[k].iter().any(|x| e.contains(x)));
                if own || meets {
                    blamed.insert(k);
                    any = true;
                }
            }
        }
        if !any {
            left += 1;
        }
    }
    Ok((blamed, left))
}

/// A joint as a run laid it out: whole, its member ends laid out and its
/// plates (all none where it is not written).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) whole: bool,
    pub(crate) ends: usize,
    pub(crate) plates: usize,
}

impl Counts {
    /// Laid out worse than `b`: fewer ends, as many on more plates, or no
    /// longer whole.
    pub(crate) fn worse_than(&self, b: &Counts) -> bool {
        self.ends < b.ends
            || (self.ends == b.ends && self.plates > b.plates)
            || (b.whole && !self.whole)
    }

    /// Laid out better than `b`: more ends, as many on fewer plates, or
    /// whole where it was not.
    fn better_than(&self, b: &Counts) -> bool {
        self.ends > b.ends
            || (self.ends == b.ends && self.plates < b.plates)
            || (self.whole && !b.whole)
    }
}

/// Each joint of run `r` laid out ([`Counts`]).
pub(crate) fn counts_of(r: &LayoutRun) -> BTreeMap<&str, Counts> {
    r.joints
        .iter()
        .map(|o| {
            (
                o.joint.as_str(),
                if o.written {
                    Counts {
                        whole: o.hand.is_empty(),
                        ends: o.plan.ends.len(),
                        plates: o.plates.len(),
                    }
                } else {
                    Counts::default()
                },
            )
        })
        .collect()
}

/// The run's guard over the joints a turn reaches (before, after): at none
/// laid out worse ([`Counts::worse_than`]); in all more ends, or as many on
/// fewer plates, or as many joints whole more.
pub(crate) fn helps(before: &[Counts], after: &[Counts]) -> bool {
    if before.iter().zip(after).any(|(b, a)| a.worse_than(b)) {
        return false;
    }
    before.iter().zip(after).any(|(b, a)| a.better_than(b))
}

/// Each member's roll in the structure file `text`, as written.
fn rolls_in(text: &str) -> Result<BTreeMap<String, String>, String> {
    let doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let mut out = BTreeMap::new();
    if let Some(a) = doc
        .get("member")
        .and_then(toml_edit::Item::as_array_of_tables)
    {
        for m in a {
            if let Some(id) = m.get("id").and_then(toml_edit::Item::as_str) {
                let roll = m
                    .get("roll")
                    .and_then(toml_edit::Item::as_value)
                    .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
                    .unwrap_or_default();
                out.insert(id.to_owned(), roll);
            }
        }
    }
    Ok(out)
}

/// The run's notes on the turns `list` weighed: how many kept, then each.
pub fn notes_of(list: &[WeighedTurn]) -> Vec<String> {
    let (k, n) = (
        list.iter()
            .filter(|w| w.kept && !is_flange(&w.kind))
            .count(),
        list.iter().filter(|w| !is_flange(&w.kind)).count(),
    );
    let mut out = vec![format!(
        "the turns from the joints' planes weighed over every joint they reach: {k} of {n} \
         kept, {} taken back (offered in Lay out joints)",
        n - k
    )];
    out.extend(list.iter().map(WeighedTurn::words));
    out
}

/// A turn of kind `kind` is L25's ([`crate::flange_turns`]): half or a
/// quarter turn for an outstanding leg pointing into the flange.
pub fn is_flange(kind: &str) -> bool {
    matches!(kind, "half turn" | "quarter turn" | "flange turn")
}

/// A roll value as written (empty: none).
pub(crate) fn roll_text(v: Option<&toml::Value>) -> String {
    match v {
        None => String::new(),
        Some(toml::Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// A roll as written in degrees (none, or not read: 0).
fn deg(text: &str) -> f64 {
    roll_radians(text).to_degrees()
}

/// A roll as written in radians (none, or not read: 0).
pub(crate) fn roll_radians(text: &str) -> f64 {
    if text.is_empty() {
        return 0.0;
    }
    let v: toml::Value = text
        .parse::<f64>()
        .map(toml::Value::Float)
        .unwrap_or_else(|_| toml::Value::String(text.to_owned()));
    v.try_into::<ohl_core::units::Angle>()
        .map_or(0.0, |a| a.radians())
}

/// A roll of `rad` radians as the files write it (as
/// `ohl_structure_edit::edits::set_member_roll`: degrees to a hundredth,
/// within ±180°; empty at 0).
pub(crate) fn roll_written(rad: f64) -> String {
    let mut deg = rad.to_degrees() % 360.0;
    if deg > 180.0 {
        deg -= 360.0;
    } else if deg <= -180.0 {
        deg += 360.0;
    }
    let deg = (deg * 100.0).round() / 100.0;
    if deg == 0.0 {
        String::new()
    } else {
        format!("{deg} deg")
    }
}

/// A few joints in words ("J1, J2, …").
fn list(js: &[String]) -> String {
    let mut s = js.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
    if js.len() > 5 {
        s.push_str(", …");
    }
    s
}
