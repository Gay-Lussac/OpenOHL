// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Each turn the far-out welded plates change, decided with and without
//! them** (the automatic joint layout's L28; L23's question 3, the owner's
//! "use recommendations for l23 to l25" of 2026-11-25). Built, in active
//! development, not approved yet.
//!
//! L20 item 2 lays an end out on a plate in its own plane welded to the
//! flat plate even where that puts its axis up to 40 mm farther out (the
//! offset recorded for the analysis). The run chooses its half and quarter
//! turns (steps 2b, L3b) and L10's other faces by counting the ends its
//! plans lay out at all the member's joints — with those plates in the
//! counts. An angle has one roll along its whole length: a turn the welded
//! plates make look better at one joint can cost another joint of the same
//! member (L20 measured such joints left worse), and a turn they make look
//! worse can be missed. L23 tried the turns chosen without them for every
//! member: worse overall (the welded plates gain mostly through turns).
//!
//! Here, after the run (and L19's weighing), the tower is laid out once
//! more with those turns chosen **without** the far-out welded plates
//! ([`crate::LayoutSettings::turns_without_own`]; the joints then planned
//! with them where the rolls let them, as always). Every member whose roll
//! that changes — the turns the welded plates change — is decided over
//! **every joint it reaches**, after writing, as L19 weighs Orient angles'
//! turns ([`crate::weighed_turns`]): the roll chosen **with** them is kept
//! only where each of those joints is laid out no worse than without them
//! and one better (more ends, as many on fewer plates, or whole where it
//! was not), and no joint elsewhere is worse where a member turned
//! otherwise meets the member's joints; else the roll chosen **without**
//! them (on a tie too: the plates then gain nothing for the offset they
//! put into the analysis — L20's tie rule). The rolls without them are laid
//! out together and the members kept with them weighed again until every
//! one still helps ([`crate::weighed_turns::WEIGH_ROUNDS`] at most; none
//! kept with them where they do not agree by then).
//!
//! Each member decided is said in the run's notes (both rolls, its joints'
//! ends and plates either way, which was kept and why) and recorded in the
//! layout record's `own_turns`: a roll kept without them is held by later
//! runs (never turned again, as L25's turns); a row still current is not
//! decided again, so a run again on the result changes nothing. Members
//! one part with another (O44 L32, the owner's "go ahead" of 2026-11-27)
//! are decided **as a whole part at once**, as L19 weighs a run: over every
//! joint any of its members reaches, every member at its roll with the
//! plates or every member at its roll without them (held), never apart; the
//! rolls either way are the run's (where they lie more than
//! [`PART_ALIKE_DEG`] apart along the part, said); a part with a member
//! reaching a joint the run does not lay out is left with the roll chosen
//! with the plates, said. Where the roll is the
//! designer's the run turns nothing, so nothing differs. Nothing here
//! changes a member's size, work point or what is checked: it chooses
//! between two rolls the run would write anyway.

use std::collections::{BTreeMap, BTreeSet};

use ohl_structures::{LibraryItem, StructureType};
use toml_edit::DocumentMut;

use crate::LayoutSettings;
use crate::layout_run::{LayoutRecord, LayoutRun, RunAsk, lay_out_plain, put_roll};
use crate::weighed_turns::{
    Counts, TurnCandidate, TurnRoll, WEIGH_ROUNDS, counts_of, helps, knock_on, roll_text, rolls_in,
};

/// A row's kind: the roll chosen with the far-out welded plates kept (the
/// run's own choice; not held).
pub const WITH: &str = "with";
/// A row's kind: the roll chosen without them kept (held by later runs).
pub const WITHOUT: &str = "without";

/// A member whose turn the far-out welded plates change, decided (the
/// layout record's `own_turns`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnTurnRow {
    /// Its roll before the run, as written (empty: none).
    pub before: String,
    /// Its roll with the turns chosen with the far-out welded plates, as
    /// written.
    pub with: String,
    /// Its roll with them chosen without those plates, as written.
    pub without: String,
    /// [`WITH`] or [`WITHOUT`]: which was kept.
    pub kind: String,
    /// In words: both rolls, the effect either way, which was kept and why.
    pub words: String,
}

impl OwnTurnRow {
    /// The roll kept, as written.
    pub fn kept(&self) -> &str {
        if self.kind == WITHOUT {
            &self.without
        } else {
            &self.with
        }
    }

    /// The roll kept is held by a run (the roll chosen without the plates).
    pub fn held(&self) -> bool {
        self.kind == WITHOUT
    }

    /// Still the member's: its roll in `t` is the roll kept.
    pub fn current(&self, t: &StructureType, m: &str) -> bool {
        t.members
            .iter()
            .find(|x| x.id == m)
            .is_some_and(|x| roll_text(x.roll.as_ref()) == self.kept())
    }
}

/// The weighing is asked by `s`: on, the far-out welded plates laid out,
/// and not the run that chooses the turns without them.
pub fn asked(s: &LayoutSettings) -> bool {
    s.own_turns_weighed && s.own_offset_analysed && !s.turns_without_own
}

/// A member of a candidate: its rolls before, with and without the
/// far-out welded plates, as written.
struct CandMember {
    member: String,
    before: String,
    with: String,
    without: String,
}

/// A member to decide — or (O44 L32) a whole part: members one part with
/// another (`continuous`), decided at once.
struct Candidate {
    /// The member, or the part's members in the file's order.
    members: Vec<CandMember>,
    /// The joints any of them reaches (both ends of each).
    reach: Vec<String>,
}

impl Candidate {
    /// The member's id, or the part's first.
    fn member(&self) -> &str {
        &self.members[0].member
    }

    /// Its members' ids, joined.
    fn ids(&self) -> String {
        self.members
            .iter()
            .map(|c| c.member.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// **Decide the turns the far-out welded plates change** (see the module),
/// then L25's half and quarter turns ([`crate::flange_turns`]) on the file
/// so settled: the run and the file before it settled (the rolls kept
/// without the plates written, every row recorded). `r0`: the run on
/// `text`; `base_notes`: its notes that are not its plain run's (L19's
/// weighing), kept where the run is laid out again.
///
/// In rounds: L25's kept turns change the layout of their joints, and the
/// turns chosen without the plates may then write another member's roll
/// otherwise (found on an invented body with a plate bent once carrying its
/// offset on too: a run again decided more) — so where L25 turns a member,
/// both are made again on the file so settled ([`ROUNDS`] at most), until a
/// round's L25 turns none.
pub(crate) fn weigh_own_then_flange(
    text: &str,
    r0: LayoutRun,
    base_notes: &[String],
    ask: &RunAsk<'_>,
) -> Result<(LayoutRun, String), String> {
    let mut extra = base_notes.to_vec();
    let mut run = r0;
    let mut settled = text.to_owned();
    for round in 0..ROUNDS {
        let (r, s, head) = weigh_own_turns(&settled, run, &extra, ask)?;
        extra.extend(head);
        let before = rolls_in(&s)?;
        let (r, s, head) = crate::flange_turns::weigh_flange_turns(&s, r, &extra, ask)?;
        extra.extend(head);
        run = r;
        let turned = rolls_in(&s)? != before;
        settled = s;
        if !turned || !asked(ask.settings) {
            break;
        }
        if round + 1 == ROUNDS {
            run.notes.push(format!(
                "the turns the far-out welded plates change (L28) and L25's turns: members \
                 turned in each of {ROUNDS} rounds; any the last round's turns change are not \
                 decided (a run again decides them)"
            ));
        }
    }
    Ok((run, settled))
}

/// The most rounds of L28's and L25's weighing together (the program's
/// bound): where L25 turns a member, both are made again on the settled
/// file, at most this many times.
pub const ROUNDS: usize = 3;

/// The members whose roll the run `r0` (with the far-out welded plates)
/// and `rb` (the turns chosen without them) write otherwise, to decide —
/// each alone, or (O44 L32) with every member one part with it (written
/// `continuous` in the file or either run) as one: the whole part at once.
/// And the parts not decided (a member reaching a joint not asked).
fn candidates(
    text: &str,
    r0: &LayoutRun,
    rb: &LayoutRun,
    targets: &BTreeSet<String>,
) -> Result<(Vec<Candidate>, Vec<String>), String> {
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let rec = LayoutRecord::of(&t);
    let (ra, rbr) = (rolls_in(&r0.text)?, rolls_in(&rb.text)?);
    // The pieces one part with each other, from the file and both runs.
    let mut joined: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for x in [text, r0.text.as_str(), rb.text.as_str()] {
        let x = StructureType::parse(std::path::Path::new("structure.ohls"), x)
            .map_err(|e| e.message)?;
        for c in &x.connections {
            if c.kind() != ohl_structures::connections::ConnectionKind::Continuous {
                continue;
            }
            let Some(onto) = &c.onto else { continue };
            joined
                .entry(c.member.clone())
                .or_default()
                .insert(onto.clone());
            joined
                .entry(onto.clone())
                .or_default()
                .insert(c.member.clone());
        }
    }
    let order: BTreeMap<&str, usize> = t
        .members
        .iter()
        .enumerate()
        .map(|(i, m)| (m.id.as_str(), i))
        .collect();
    let nodes_of: BTreeMap<&str, &[String; 2]> = t
        .members
        .iter()
        .map(|m| (m.id.as_str(), &m.nodes))
        .collect();
    let now = |m: &str| -> String {
        t.members
            .iter()
            .find(|x| x.id == m)
            .map(|x| roll_text(x.roll.as_ref()))
            .unwrap_or_default()
    };
    // The part of `m`: every member one part with it, in the file's order.
    let part_of = |m: &str| -> Vec<String> {
        let mut seen: BTreeSet<String> = [m.to_owned()].into();
        let mut todo = vec![m.to_owned()];
        while let Some(x) = todo.pop() {
            for y in joined.get(&x).into_iter().flatten() {
                if order.contains_key(y.as_str()) && seen.insert(y.clone()) {
                    todo.push(y.clone());
                }
            }
        }
        let mut v: Vec<String> = seen.into_iter().collect();
        v.sort_by_key(|x| order.get(x.as_str()).copied().unwrap_or(usize::MAX));
        v
    };
    let rolls = |m: &str| -> (String, String) {
        (
            ra.get(m).cloned().unwrap_or_else(|| now(m)),
            rbr.get(m).cloned().unwrap_or_else(|| now(m)),
        )
    };
    let decided = |m: &str| rec.own_turns.get(m).is_some_and(|r| r.current(&t, m));
    let mut out = Vec::new();
    let mut left = Vec::new();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for m in &t.members {
        let (Some(with), Some(without)) = (ra.get(&m.id), rbr.get(&m.id)) else {
            continue;
        };
        if with == without || taken.contains(&m.id) || decided(&m.id) {
            continue;
        }
        let part = part_of(&m.id);
        taken.extend(part.iter().cloned());
        let reach: BTreeSet<String> = part
            .iter()
            .filter_map(|x| nodes_of.get(x.as_str()))
            .flat_map(|n| n.iter().cloned())
            .collect();
        if !reach.iter().all(|n| targets.contains(n)) {
            // (A member alone so: not decided, as before L32; a part said.)
            if part.len() > 1 {
                left.push(part.join(", "));
            }
            continue;
        }
        // (A part whose every member turned otherwise is decided already,
        // and still so: not decided again.)
        if part.iter().all(|x| {
            let (w, o) = rolls(x);
            w == o || decided(x)
        }) {
            continue;
        }
        let members = part
            .iter()
            .map(|x| {
                let (with, without) = rolls(x);
                CandMember {
                    member: x.clone(),
                    before: rec.rolled.get(x).cloned().unwrap_or_else(|| now(x)),
                    with,
                    without,
                }
            })
            .collect();
        out.push(Candidate {
            members,
            reach: reach.into_iter().collect(),
        });
    }
    Ok((out, left))
}

/// The file `text` with the members `held` given their roll chosen without
/// the far-out welded plates (written, held by the run: the layout's own
/// turn of it taken out of the record) and the rows `rows` written.
fn text_with(
    text: &str,
    cands: &[Candidate],
    held: &BTreeSet<usize>,
    rows: &BTreeMap<String, OwnTurnRow>,
) -> Result<String, String> {
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let mut rec = LayoutRecord::of(&t);
    for (m, r) in rows {
        rec.own_turns.insert(m.clone(), r.clone());
    }
    for &k in held {
        // (A part: every member at its roll without the plates, held.)
        for c in &cands[k].members {
            put_roll(&mut doc, &c.member, &c.without);
            rec.rolled.remove(&c.member);
            let row = rows.get(&c.member).cloned().unwrap_or_else(|| OwnTurnRow {
                before: c.before.clone(),
                with: c.with.clone(),
                without: c.without.clone(),
                kind: WITHOUT.to_owned(),
                words: String::new(),
            });
            rec.own_turns.insert(
                c.member.clone(),
                OwnTurnRow {
                    kind: WITHOUT.to_owned(),
                    ..row
                },
            );
        }
    }
    rec.write(&mut doc);
    Ok(doc.to_string())
}

/// The file `text` with the record's `own_turns` rows `rows` written (each
/// replacing the member's row there).
fn rewrite_rows(text: &str, rows: &BTreeMap<String, OwnTurnRow>) -> Result<String, String> {
    if rows.is_empty() {
        return Ok(text.to_owned());
    }
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let mut rec = LayoutRecord::of(&t);
    for (m, r) in rows {
        rec.own_turns.insert(m.clone(), r.clone());
    }
    rec.write(&mut doc);
    Ok(doc.to_string())
}

/// The most passes the weighing makes (the program's bound): each lays
/// the tower out once more with the turns chosen without the far-out
/// welded plates, on the file the passes before settled.
pub const OWN_PASSES: usize = 3;

/// The weighing (see the module) in passes: the rolls decided change the
/// layout of their joints, and the turns chosen without the plates may
/// then write another member's roll otherwise (measured on an invented
/// body: one more member) — decided in a further pass on the file so
/// settled, until a pass finds none ([`OWN_PASSES`] at most), so that a run
/// again on the result decides nothing more. The run, the file settled and
/// the notes the passes added.
fn weigh_own_turns(
    text: &str,
    r0: LayoutRun,
    base_notes: &[String],
    ask: &RunAsk<'_>,
) -> Result<(LayoutRun, String, Vec<String>), String> {
    if !asked(ask.settings) {
        return Ok((r0, text.to_owned(), Vec::new()));
    }
    let mut notes = base_notes.to_vec();
    let mut heads = Vec::new();
    let mut run = r0;
    let mut settled = text.to_owned();
    for pass in 0..OWN_PASSES {
        let (r, s, head, decided) = weigh_own_pass(&settled, run, &notes, ask)?;
        run = r;
        settled = s;
        notes.extend(head.iter().cloned());
        heads.extend(head);
        if !decided {
            return Ok((run, settled, heads));
        }
        if pass + 1 == OWN_PASSES {
            let w = format!(
                "the turns the far-out welded plates change (L28): rolls held without those \
                 plates in each of {OWN_PASSES} passes; any member the last pass's rolls change \
                 is not decided (a run again decides it)"
            );
            run.notes.push(w.clone());
            heads.push(w);
        }
    }
    Ok((run, settled, heads))
}

/// One pass of [`weigh_own_turns`]: the run, the file settled, the notes
/// it added and whether it held any member at its roll without the plates
/// (none decided: `r0` and `text` as they are, its note said).
fn weigh_own_pass(
    text: &str,
    r0: LayoutRun,
    base_notes: &[String],
    ask: &RunAsk<'_>,
) -> Result<(LayoutRun, String, Vec<String>, bool), String> {
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let targets: BTreeSet<String> = ask
        .joints
        .clone()
        .unwrap_or_else(|| t.nodes.iter().map(|n| n.id.clone()).collect());
    // 1. The tower laid out with the turns chosen without the far-out
    // welded plates.
    let without_s = LayoutSettings {
        turns_without_own: true,
        ..ask.settings.clone()
    };
    let without_ask = RunAsk {
        settings: &without_s,
        ..ask.clone()
    };
    let rb = lay_out_plain(text, &without_ask)?;
    let mut seconds = rb.seconds;
    let mut layouts = 1usize;
    let (cands, left) = candidates(text, &r0, &rb, &targets)?;
    if cands.is_empty() {
        let mut out = r0;
        out.seconds += seconds;
        let mut head = vec![format!(
            "the turns the far-out welded plates change (L28): none to decide — the tower laid \
             out with the turns chosen without those plates writes no member's roll otherwise \
             that may be decided here; the weighing laid the tower out 1 more time ({seconds:.1} \
             s, measured)"
        )];
        if !left.is_empty() {
            head.push(own_part_words(&left));
        }
        out.notes.extend(head.iter().cloned());
        return Ok((out, text.to_owned(), head, false));
    }
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
    let reach: Vec<Vec<String>> = cands.iter().map(|c| c.reach.clone()).collect();
    // As the L19 weighing takes them: the roll with the plates the turn,
    // the roll without them before it (one member each).
    // (A part: its members turned as one, as L19 weighs a run.)
    let as_turns: Vec<TurnCandidate> = cands
        .iter()
        .map(|c| TurnCandidate {
            run: c.member().to_owned(),
            kind: "own".to_owned(),
            stage: 2,
            members: c
                .members
                .iter()
                .map(|x| TurnRoll {
                    member: x.member.clone(),
                    before: x.without.clone(),
                    with: x.with.clone(),
                })
                .collect(),
        })
        .collect();
    let at = |r: &LayoutRun, k: usize| -> Vec<Counts> {
        let by = counts_of(r);
        reach[k]
            .iter()
            .map(|j| by.get(j.as_str()).copied().unwrap_or_default())
            .collect()
    };
    let sum = |v: &[Counts]| {
        v.iter()
            .fold((0usize, 0usize), |(e, p), c| (e + c.ends, p + c.plates))
    };
    let worse_at = |b: &[Counts], a: &[Counts], k: usize| -> Vec<String> {
        b.iter()
            .zip(a)
            .zip(&reach[k])
            .filter(|((b, a), _)| a.worse_than(b))
            .map(|(_, j)| j.clone())
            .collect()
    };
    // 2. The rolls with the plates kept where they help over every joint
    // the member reaches against the layout without them (rb, the base);
    // the others held at their roll without them, laid out together, and
    // those kept weighed again until they agree.
    let all: BTreeSet<usize> = (0..cands.len()).collect();
    let (blamed0, _) = knock_on(&rb, &r0, &all, &reach, &as_turns, &at_joint, &nodes_of)?;
    let first: BTreeSet<usize> = all
        .iter()
        .copied()
        .filter(|&k| !blamed0.contains(&k) && helps(&at(&rb, k), &at(&r0, k)))
        .collect();
    // (Each member's effect as first weighed: with, without — its joints'
    // ends and plates, and the joints worse with the plates.)
    let first_with: Vec<(usize, usize)> = (0..cands.len()).map(|k| sum(&at(&r0, k))).collect();
    let first_without: Vec<(usize, usize)> = (0..cands.len()).map(|k| sum(&at(&rb, k))).collect();
    let first_worse: Vec<Vec<String>> = (0..cands.len())
        .map(|k| worse_at(&at(&rb, k), &at(&r0, k), k))
        .collect();
    let mut with_kept = first.clone();
    let mut last: Option<LayoutRun> = None;
    let mut agreed = with_kept == all;
    if !agreed {
        for round in 0..=WEIGH_ROUNDS {
            let held: BTreeSet<usize> = all.difference(&with_kept).copied().collect();
            let r = lay_out_plain(&text_with(text, &cands, &held, &BTreeMap::new())?, ask)?;
            seconds += r.seconds;
            layouts += 1;
            let (blamed, _) =
                knock_on(&rb, &r, &with_kept, &reach, &as_turns, &at_joint, &nodes_of)?;
            let keep: BTreeSet<usize> = with_kept
                .iter()
                .copied()
                .filter(|&k| !blamed.contains(&k) && helps(&at(&rb, k), &at(&r, k)))
                .collect();
            last = Some(r);
            if keep == with_kept {
                agreed = true;
                break;
            }
            with_kept = keep;
            if round == WEIGH_ROUNDS {
                break;
            }
        }
        if !agreed {
            // (None kept with the plates where they do not agree by then:
            // every member at its roll without them.)
            with_kept.clear();
            let r = lay_out_plain(&text_with(text, &cands, &all, &BTreeMap::new())?, ask)?;
            seconds += r.seconds;
            layouts += 1;
            last = Some(r);
        }
    }
    // 3. The rows: each member decided, in words.
    let fin: &LayoutRun = last.as_ref().unwrap_or(&r0);
    // (A member kept with the plates as the last layout wrote it: the run
    // chooses its roll again there, with the others' rolls decided.)
    let fin_rolls = rolls_in(&fin.text)?;
    let mut rows: BTreeMap<String, OwnTurnRow> = BTreeMap::new();
    let mut notes = Vec::new();
    let (mut n_with, mut n_without) = (0usize, 0usize);
    for (k, c) in cands.iter().enumerate() {
        let joints = c.reach.join(", ");
        let (ew, pw) = first_with[k];
        let (eo, po) = first_without[k];
        let (kind, why) = if with_kept.contains(&k) {
            n_with += 1;
            (
                WITH,
                "the roll with them kept: every joint it reaches laid out no worse with them \
                 and one better"
                    .to_owned(),
            )
        } else {
            n_without += 1;
            let reason = if !first.contains(&k) {
                if !first_worse[k].is_empty() {
                    format!(
                        "joint(s) {} laid out worse with them",
                        first_worse[k].join(", ")
                    )
                } else if blamed0.contains(&k) {
                    "a joint elsewhere laid out worse with them where a member turned otherwise \
                     meets its joints"
                        .to_owned()
                } else {
                    "no joint laid out better with them (a tie: the plates' offset in the \
                     analysis for nothing)"
                        .to_owned()
                }
            } else if agreed {
                "with the other members decided, its joints no longer all laid out better \
                 with them"
                    .to_owned()
            } else {
                format!(
                    "the rolls kept with them did not all still help after {} layouts",
                    WEIGH_ROUNDS + 1
                )
            };
            (
                WITHOUT,
                format!("the roll without them kept ({reason}); held by later runs"),
            )
        };
        let (ef, pf) = sum(&at(fin, k));
        let effect = format!(
            "{joints} lay out {ew} member end(s) on {pw} plate(s) with them, {eo} on {po} \
             without them; {why}; as written: {ef} end(s) on {pf} plate(s)"
        );
        // A member alone: its words as before (O44 L28); a part (O44 L32):
        // its members and their rolls either way, decided at once.
        let (note, row_words): (String, Vec<String>) = match c.members.as_slice() {
            [x] => {
                let w = format!(
                    "{}: its turn chosen with the far-out welded plates (L20 item 2) writes roll \
                     {}, chosen without them {} — its joints {effect}",
                    x.member,
                    shown(&x.with),
                    shown(&x.without),
                );
                (w.clone(), vec![w])
            }
            xs => {
                let rolls = |f: fn(&CandMember) -> &str| -> String {
                    let first = f(&xs[0]);
                    if xs.iter().all(|x| f(x) == first) {
                        return format!("roll {} for all", shown(first));
                    }
                    let each = xs
                        .iter()
                        .map(|x| format!("{} {}", x.member, shown(f(x))))
                        .collect::<Vec<_>>()
                        .join(", ");
                    // (Said where the run writes the pieces' rolls more
                    // than 2° apart: not alike along the part.)
                    let apart = apart_deg(xs.iter().map(f));
                    if apart > PART_ALIKE_DEG {
                        format!("{each} (not alike along the part: {apart:.0}° apart)")
                    } else {
                        each
                    }
                };
                let what = format!(
                    "one part, decided at once (O44 L32): their turns chosen with the far-out \
                     welded plates (L20 item 2) write {}, chosen without them {} — the joints any \
                     of them reaches, {effect}",
                    rolls(|x| x.with.as_str()),
                    rolls(|x| x.without.as_str()),
                );
                let ids = c.ids();
                let rows: Vec<String> = xs
                    .iter()
                    .map(|x| {
                        let others: Vec<&str> = xs
                            .iter()
                            .filter(|y| y.member != x.member)
                            .map(|y| y.member.as_str())
                            .collect();
                        format!("{}: with {}, {what}", x.member, others.join(", "))
                    })
                    .collect();
                (format!("{ids}: {what}"), rows)
            }
        };
        notes.push(note);
        for (x, words) in c.members.iter().zip(row_words) {
            let with = if kind == WITH {
                fin_rolls
                    .get(&x.member)
                    .cloned()
                    .unwrap_or_else(|| x.with.clone())
            } else {
                x.with.clone()
            };
            rows.insert(
                x.member.clone(),
                OwnTurnRow {
                    before: x.before.clone(),
                    with,
                    without: x.without.clone(),
                    kind: kind.to_owned(),
                    words,
                },
            );
        }
    }
    let held: BTreeSet<usize> = all.difference(&with_kept).copied().collect();
    let settled = text_with(text, &cands, &held, &rows)?;
    let mut out = match last {
        Some(r) => {
            let text1 = rewrite_rows(&r.text, &rows)?;
            let mut notes = base_notes.to_vec();
            notes.extend(r.notes);
            LayoutRun {
                text: text1,
                notes,
                weighed: r0.weighed.clone(),
                ..r
            }
        }
        None => {
            let text1 = rewrite_rows(&r0.text, &rows)?;
            LayoutRun {
                text: text1,
                ..r0.clone()
            }
        }
    };
    out.seconds = r0.seconds + seconds;
    let parts = cands.iter().filter(|c| c.members.len() > 1).count();
    let in_parts: usize = cands
        .iter()
        .filter(|c| c.members.len() > 1)
        .map(|c| c.members.len())
        .sum();
    let mut head = vec![format!(
        "the turns the far-out welded plates change (L28): {} member(s) decided with and \
         without those plates over every joint they reach{} — the roll with them kept at \
         {n_with}, without them at {n_without}; the weighing laid the tower out {layouts} more \
         time(s) ({seconds:.1} s, measured)",
        cands.len(),
        if parts > 0 {
            format!(
                ", {parts} of them whole parts of members one part with another ({in_parts} \
                 members, each part decided at once, O44 L32)"
            )
        } else {
            String::new()
        }
    )];
    if !left.is_empty() {
        head.push(own_part_words(&left));
    }
    head.append(&mut notes);
    out.notes.extend(head.iter().cloned());
    // (Only a roll held without the plates changes the layout a further
    // pass starts from; those kept with them are the run's own choice.)
    Ok((out, settled, head, n_without > 0))
}

/// How far apart the pieces of a part may lie in roll and still be said
/// alike, degrees (O44 L32; J9's 2° for pieces in line: the mean written).
pub const PART_ALIKE_DEG: f64 = 2.0;

/// The most two of `rolls` (as written) lie apart, degrees (0 to 180).
fn apart_deg<'a>(rolls: impl Iterator<Item = &'a str>) -> f64 {
    let r: Vec<f64> = rolls
        .map(|x| crate::weighed_turns::roll_radians(x).to_degrees())
        .collect();
    let mut most = 0.0_f64;
    for (i, a) in r.iter().enumerate() {
        for b in &r[i + 1..] {
            let d = (a - b).rem_euclid(360.0);
            most = most.max(d.min(360.0 - d));
        }
    }
    most
}

/// A roll as said (none: "none").
fn shown(roll: &str) -> &str {
    if roll.is_empty() { "none" } else { roll }
}

/// The parts (members one part with another) whose roll the far-out welded
/// plates change, left with the roll chosen with them because a member
/// reaches a joint the run does not lay out (O44 L32: a part is decided
/// over every joint any of its members reaches), in words.
fn own_part_words(left: &[String]) -> String {
    format!(
        "the turns the far-out welded plates change (L28): {} part(s) of members one part with \
         another left with the roll chosen with those plates (not decided: a member reaches a \
         joint not laid out by this run): {}",
        left.len(),
        left.join("; ")
    )
}

#[cfg(test)]
mod tests {
    /// The pieces' rolls apart, degrees: across 180° and none as 0°.
    #[test]
    fn rolls_apart() {
        let d = super::apart_deg(["90.79 deg", "-89.24 deg"].into_iter());
        assert!((d - 179.97).abs() < 1e-6, "{d}");
        let d = super::apart_deg(["179.21 deg", "-179.5 deg", "180 deg"].into_iter());
        assert!((d - 1.29).abs() < 1e-6, "{d}");
        assert_eq!(super::apart_deg(["", "0 deg"].into_iter()), 0.0);
    }
}
