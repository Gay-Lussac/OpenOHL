// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **An outstanding leg pointing into the flange: a half or a quarter
//! turn, weighed** (the automatic joint layout's L25; the owner,
//! 2026-11-24: "probably a half turn if possible then other options" and
//! "maybe a 90 degrees turn might be better so you could also consider
//! it"). Built, in active development, not approved yet.
//!
//! A plate in an end's own plane welded to the flat plate (step 4) is
//! refused where the end's outstanding leg points into the flange: it
//! would stand on the flat plate. The run's own half turn (step 2b) and,
//! for an equal-leg angle at most 20° off, its quarter turn (L3b) are
//! chosen on the plans alone. Here, after the run, every member left with
//! such an end — not a leg or chord, not one part with another, every
//! joint it reaches laid out by the run — is laid out turned by half a
//! turn and by a quarter turn either way, each turn weighed as the run
//! weighs Orient angles' turns ([`crate::weighed_turns`], L19): over
//! **every joint the member reaches**, after writing, each joint no worse
//! and one better, and no joint elsewhere worse where a member the layout
//! turned otherwise meets it. Of the turns that pass, the one laying out
//! more member ends at the member's joints is kept; on a tie the half turn
//! (the same leg bolted), then the fewer plates. The turns kept are laid
//! out again together and weighed again until every one still helps
//! ([`crate::weighed_turns::WEIGH_ROUNDS`] at most).
//!
//! A quarter turn bolts the angle's other leg: on an equal-leg angle the
//! same width; on an unequal angle the other width, with its own bolts,
//! gauges and edge distances (the plan's, from that leg), said with the
//! turn — only where neither the member's `connected_leg` nor the end's
//! `leg` says which leg is bolted (a value given is never changed; the
//! member check then takes the short leg bolted, as without one).
//!
//! Where the member's roll is the program's (no roll given, or one the
//! layout or Orient angles set; or values given by hand replaced) the turn
//! is applied: the member's roll written turned, the layout record's
//! `flange_turns` row saying the roll before and with it — a later run
//! keeps that roll (never turns it again) and does not weigh the member
//! again. Where the roll is the designer's, the turn is never applied: it
//! is offered (the row says so, with its effect), for Lay out joints to
//! apply when ticked. A member where neither turn helps is left, its row
//! saying so (the other options — a stand-off, a welded assembly of three
//! parts — later). Deterministic: the same file and settings give the same
//! result; each weighing is three whole layouts and one or more to
//! confirm (measured and said in the run's notes).

use std::collections::{BTreeMap, BTreeSet};

use ohl_structures::{LibraryItem, StructureType};
use toml_edit::DocumentMut;

use crate::layout_run::{LayoutRecord, LayoutRun, RunAsk, lay_out_plain, put_roll};
use crate::weighed_turns::{
    Counts, TurnCandidate, TurnRoll, WEIGH_ROUNDS, WeighedTurn, counts_of, helps, knock_on,
    roll_radians, roll_text, roll_written,
};

/// The reason the plan gives for an end whose outstanding leg points into
/// the flange (step 4's welded plate refused).
pub const INTO_FLANGE: &str = "its outstanding leg points towards the flange";

/// A row's kind: half a turn, applied.
pub const HALF: &str = "half";
/// A row's kind: a quarter turn, applied.
pub const QUARTER: &str = "quarter";
/// A row's kind: a turn offered, not applied (the roll the designer's).
pub const OFFERED: &str = "offered";
/// A row's kind: weighed, neither turn helps (left).
pub const NONE: &str = "none";

/// A member weighed for its outstanding leg pointing into the flange (the
/// layout record's `flange_turns`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlangeTurnRow {
    /// Its roll before, as written (empty: none) — the roll a run starts
    /// from (one the layout turned before: the roll before that turn).
    pub before: String,
    /// Its roll with the turn, as written (applied: the member's roll;
    /// offered: the roll Lay out joints writes when it is ticked; none:
    /// as before).
    pub with: String,
    /// [`HALF`], [`QUARTER`], [`OFFERED`] or [`NONE`].
    pub kind: String,
    /// In words: the turn, the leg it bolts and its effect as weighed.
    pub words: String,
}

impl FlangeTurnRow {
    /// The turn is applied (the member's roll is `with`).
    pub fn applied(&self) -> bool {
        self.kind == HALF || self.kind == QUARTER
    }

    /// Still the member's as `t` gives it: applied, its roll still `with`;
    /// else its roll before a run (`rolled`: the layout's turns of the
    /// record, taken back by a run) still `before`.
    pub fn current(&self, t: &StructureType, rolled: &BTreeMap<String, String>, m: &str) -> bool {
        let Some(x) = t.members.iter().find(|x| x.id == m) else {
            return false;
        };
        let now = roll_text(x.roll.as_ref());
        if self.applied() {
            now == self.with
        } else {
            rolled.get(m).cloned().unwrap_or(now) == self.before
        }
    }
}

/// A way to turn a member about its axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Way {
    /// Half a turn (the same leg bolted).
    Half,
    /// A quarter turn one way (+90°).
    QuarterPlus,
    /// A quarter turn the other way (−90°).
    QuarterMinus,
}

impl Way {
    fn degrees(self) -> f64 {
        match self {
            Way::Half => 180.0,
            Way::QuarterPlus => 90.0,
            Way::QuarterMinus => -90.0,
        }
    }

    fn kind(self) -> &'static str {
        match self {
            Way::Half => HALF,
            _ => QUARTER,
        }
    }
}

/// A member to weigh.
struct Candidate {
    member: String,
    /// The joints where its outstanding leg points into the flange.
    at: Vec<String>,
    /// The joints it reaches (both ends).
    reach: Vec<String>,
    /// Its roll as the run started from it, as written.
    before: String,
    /// Its roll as the run wrote it, radians (the turns are added to it).
    written: f64,
    /// Its roll the program's (applied), else the designer's (offered).
    auto: bool,
    /// The ways it may be turned.
    ways: Vec<Way>,
    /// An unequal angle: its legs, m (the quarter turn bolts the other).
    legs: Option<(f64, f64)>,
}

impl Candidate {
    fn with(&self, w: Way) -> String {
        roll_written(self.written + w.degrees().to_radians())
    }

    /// The turn in words, with the leg it bolts.
    fn turn_words(&self, w: Way) -> String {
        match (w, self.legs) {
            (Way::Half, _) => "half a turn (the same leg bolted)".to_owned(),
            (_, None) => format!(
                "a quarter turn ({:+.0}°; its other leg bolted, as wide)",
                w.degrees()
            ),
            (_, Some((a, b))) => format!(
                "a quarter turn ({:+.0}°; its other leg bolted — an unequal angle {:.0} × {:.0} \
                 mm: the other width, with its own bolts, gauges and edge distances; the member \
                 check takes the short leg bolted, as no leg is given)",
                w.degrees(),
                a.max(b) * 1e3,
                a.min(b) * 1e3
            ),
        }
    }
}

/// The members of the run `r0` on `text` with an end whose outstanding leg
/// points into the flange, to weigh (see the module); `targets` the joints
/// asked.
fn candidates(
    text: &str,
    r0: &LayoutRun,
    ask: &RunAsk<'_>,
    targets: &BTreeSet<String>,
) -> Result<Vec<Candidate>, String> {
    let s = ask.settings;
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let t0 = StructureType::parse(std::path::Path::new("structure.ohls"), &r0.text)
        .map_err(|e| e.message)?;
    let rec = LayoutRecord::of(&t);
    let mut at: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for o in &r0.joints {
        for h in &o.hand {
            if let Some((m, why)) = h.split_once(": ")
                && why.contains(INTO_FLANGE)
            {
                let v = at.entry(m.to_owned()).or_default();
                if !v.contains(&o.joint) {
                    v.push(o.joint.clone());
                }
            }
        }
    }
    let point = |n: &str| {
        t.nodes
            .iter()
            .find(|x| x.id == n)
            .map(|x| x.point.map(|v| v.si()))
    };
    let mut out = Vec::new();
    for (m, joints) in at {
        let Some(x) = t.members.iter().find(|x| x.id == m) else {
            continue;
        };
        if !x.nodes.iter().all(|n| targets.contains(n)) {
            continue;
        }
        if rec
            .flange_turns
            .get(&m)
            .is_some_and(|r| r.current(&t, &rec.rolled, &m))
        {
            continue;
        }
        // Not a leg or chord.
        let class = x
            .design
            .as_ref()
            .and_then(|d| d.get("class"))
            .and_then(toml::Value::as_str);
        let dir = match (point(&x.nodes[0]), point(&x.nodes[1])) {
            (Some(a), Some(b)) => {
                let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-12);
                [d[0] / l, d[1] / l, d[2] / l]
            }
            _ => [0.0, 0.0, 1.0],
        };
        if crate::joint_layout::class_rank(class, dir) == crate::joint_layout::ClassRank::Leg
            || rec
                .oriented
                .get(&m)
                .is_some_and(|(_, k)| *k == crate::OrientKind::Chord)
        {
            continue;
        }
        // Not one part with another (given, or joined by the run).
        let one_part = t0.connections.iter().any(|c| {
            c.kind() == ohl_structures::connections::ConnectionKind::Continuous
                && (c.member == m || c.onto.as_deref() == Some(m.as_str()))
        });
        if one_part {
            continue;
        }
        let shape = x
            .section
            .as_deref()
            .and_then(|id| ask.library.section(id))
            .map(|s| &s.shape);
        let legs = match shape {
            Some(ohl_sections::Shape::EqualAngle { .. }) => None,
            Some(ohl_sections::Shape::UnequalAngle { b, h, .. }) if (b - h).abs() > 1e-9 => {
                Some((*b, *h))
            }
            Some(ohl_sections::Shape::UnequalAngle { .. }) => None,
            // Not a single angle: not turned here.
            _ => continue,
        };
        // An unequal angle's leg given (the member's or an end's): its
        // quarter turn would bolt the other — never changed silently.
        let leg_given = legs.is_some()
            && (x
                .design
                .as_ref()
                .and_then(|d| d.get("connected_leg"))
                .and_then(toml::Value::as_str)
                .is_some_and(|l| l != "none")
                || t.connections.iter().any(|c| {
                    c.member == m
                        && matches!(
                            c.leg,
                            Some(
                                ohl_structures::connections::BoltedLeg::Long
                                    | ohl_structures::connections::BoltedLeg::Short
                            )
                        )
                }));
        let mut ways = Vec::new();
        if s.flange_half {
            ways.push(Way::Half);
        }
        if s.flange_quarter && !leg_given {
            ways.extend([Way::QuarterPlus, Way::QuarterMinus]);
        }
        if ways.is_empty() {
            continue;
        }
        let automatic = rec.rolled.contains_key(&m) || rec.oriented.contains_key(&m);
        let auto = automatic
            || (x.roll.is_none() && x.reference.is_none() && x.orientation.is_none())
            || ask.replace_hand;
        let before = rec
            .rolled
            .get(&m)
            .cloned()
            .unwrap_or_else(|| roll_text(x.roll.as_ref()));
        let written = t0
            .members
            .iter()
            .find(|y| y.id == m)
            .map_or(0.0, |y| roll_radians(&roll_text(y.roll.as_ref())));
        out.push(Candidate {
            member: m,
            at: joints,
            reach: x.nodes.to_vec(),
            before,
            written,
            auto,
            ways,
            legs,
        });
    }
    Ok(out)
}

/// The file `text` with the turns `turned` (candidate, way) applied —
/// each member's roll written turned, its record row saying so, the
/// layout's own turn of it taken out of the record (the run keeps its roll)
/// — and the rows `rows` (offered or left) written.
fn text_with(
    text: &str,
    cands: &[Candidate],
    turned: &BTreeMap<usize, Way>,
    rows: &BTreeMap<String, FlangeTurnRow>,
) -> Result<String, String> {
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let mut rec = LayoutRecord::of(&t);
    for (m, r) in rows {
        rec.flange_turns.insert(m.clone(), r.clone());
    }
    for (&k, &w) in turned {
        let c = &cands[k];
        let with = c.with(w);
        put_roll(&mut doc, &c.member, &with);
        rec.rolled.remove(&c.member);
        rec.flange_turns.insert(
            c.member.clone(),
            FlangeTurnRow {
                before: c.before.clone(),
                with,
                kind: w.kind().to_owned(),
                words: rows
                    .get(&c.member)
                    .map(|r| r.words.clone())
                    .unwrap_or_default(),
            },
        );
    }
    rec.write(&mut doc);
    Ok(doc.to_string())
}

/// **The half and quarter turns weighed** for the members of the run `r0`
/// (laid out on `text`, the file before the run) with an end whose
/// outstanding leg points into the flange (see the module): the run with
/// the turns kept and the file before it so settled (the rolls kept, the
/// rows written). Nothing to weigh: `r0` and `text` as they are.
/// `base_notes`: the notes of `r0` that are not its plain run's (L19's
/// weighing), kept where the run is laid out again.
pub(crate) fn weigh_flange_turns(
    text: &str,
    r0: LayoutRun,
    base_notes: &[String],
    ask: &RunAsk<'_>,
) -> Result<(LayoutRun, String), String> {
    let s = ask.settings;
    if !(s.flange_half || s.flange_quarter) {
        return Ok((r0, text.to_owned()));
    }
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let targets: BTreeSet<String> = ask
        .joints
        .clone()
        .unwrap_or_else(|| t.nodes.iter().map(|n| n.id.clone()).collect());
    let cands = candidates(text, &r0, ask, &targets)?;
    if cands.is_empty() {
        return Ok((r0, text.to_owned()));
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
    // As the L19 weighing takes them (one member each).
    let as_turns = |turned: &BTreeMap<usize, Way>| -> Vec<TurnCandidate> {
        cands
            .iter()
            .enumerate()
            .map(|(k, c)| TurnCandidate {
                run: c.member.clone(),
                kind: "flange".to_owned(),
                stage: 2,
                members: vec![TurnRoll {
                    member: c.member.clone(),
                    before: c.before.clone(),
                    with: turned
                        .get(&k)
                        .map_or_else(|| c.before.clone(), |w| c.with(*w)),
                }],
            })
            .collect()
    };
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
    let none_rows = BTreeMap::new();
    let mut seconds = 0.0;
    let mut layouts = 0usize;
    // 1. Each way: every member that may take it turned so, laid out.
    // (member → the ways helping its own joints: (ends, plates), blamed).
    let mut passing: BTreeMap<usize, Vec<(Way, (usize, usize), bool)>> = BTreeMap::new();
    // (Each way tried: its ends and plates, its joints worse, and whether
    // a joint elsewhere was worse where a member turned otherwise meets.)
    type Tried = (Way, (usize, usize), Vec<String>, bool);
    let mut tried_with: BTreeMap<usize, Vec<Tried>> = BTreeMap::new();
    for way in [Way::Half, Way::QuarterPlus, Way::QuarterMinus] {
        let turned: BTreeMap<usize, Way> = cands
            .iter()
            .enumerate()
            .filter(|(_, c)| c.ways.contains(&way))
            .map(|(k, _)| (k, way))
            .collect();
        if turned.is_empty() {
            continue;
        }
        let r = lay_out_plain(&text_with(text, &cands, &turned, &none_rows)?, ask)?;
        seconds += r.seconds;
        layouts += 1;
        let tried: BTreeSet<usize> = turned.keys().copied().collect();
        let (blamed, _) = knock_on(
            &r0,
            &r,
            &tried,
            &reach,
            &as_turns(&turned),
            &at_joint,
            &nodes_of,
        )?;
        for &k in &tried {
            let (b, a) = (at(&r0, k), at(&r, k));
            tried_with.entry(k).or_default().push((
                way,
                sum(&a),
                worse_at(&b, &a, k),
                blamed.contains(&k),
            ));
            // (A joint elsewhere worse where a member turned otherwise
            // meets: with every candidate turned at once often another
            // candidate's doing — such a turn still goes on to be weighed
            // with only the turns chosen, where the guard decides.)
            if helps(&b, &a) {
                passing
                    .entry(k)
                    .or_default()
                    .push((way, sum(&a), blamed.contains(&k)));
            }
        }
    }
    // 2. Each member's best way: one no joint elsewhere blames first, then
    // the most ends, then the half turn, then the fewer plates, then +90°
    // before −90°. (Offered only where none blames it: its effect said is
    // then the whole of it.)
    let best: BTreeMap<usize, Way> = passing
        .iter()
        .filter_map(|(&k, v)| {
            v.iter()
                .filter(|(_, _, blamed)| cands[k].auto || !blamed)
                .min_by_key(|(w, (e, p), blamed)| {
                    (*blamed, std::cmp::Reverse(*e), *w != Way::Half, *p, *w)
                })
                .map(|(w, _, _)| (k, *w))
        })
        .collect();
    // 3. The program's rolls: the best turns laid out together, each kept
    // while every joint it reaches still lays out better (weighed again
    // with those still kept until they agree).
    let mut tried: BTreeMap<usize, Way> = best
        .iter()
        .filter(|(k, _)| cands[**k].auto)
        .map(|(k, w)| (*k, *w))
        .collect();
    let mut kept: BTreeMap<usize, Way> = BTreeMap::new();
    let mut last: Option<LayoutRun> = None;
    let mut after: BTreeMap<usize, (Vec<Counts>, Vec<String>)> = BTreeMap::new();
    for round in 0..=WEIGH_ROUNDS {
        if tried.is_empty() {
            break;
        }
        let r = lay_out_plain(&text_with(text, &cands, &tried, &none_rows)?, ask)?;
        seconds += r.seconds;
        layouts += 1;
        let set: BTreeSet<usize> = tried.keys().copied().collect();
        let (blamed, _) = knock_on(
            &r0,
            &r,
            &set,
            &reach,
            &as_turns(&tried),
            &at_joint,
            &nodes_of,
        )?;
        for &k in &set {
            let (b, a) = (at(&r0, k), at(&r, k));
            let w = worse_at(&b, &a, k);
            after.insert(k, (a, w));
        }
        let keep: BTreeMap<usize, Way> = tried
            .iter()
            .filter(|(k, _)| !blamed.contains(k) && helps(&at(&r0, **k), &at(&r, **k)))
            .map(|(k, w)| (*k, *w))
            .collect();
        if keep == tried {
            kept = keep;
            last = Some(r);
            break;
        }
        if round == WEIGH_ROUNDS {
            break;
        }
        tried = keep;
    }
    // 4. The rows: each member weighed, in words.
    let mut rows: BTreeMap<String, FlangeTurnRow> = BTreeMap::new();
    let mut weighed: Vec<WeighedTurn> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let (mut n_half, mut n_quarter, mut n_offered, mut n_none) = (0, 0, 0, 0);
    for (k, c) in cands.iter().enumerate() {
        let b = sum(&at(&r0, k));
        let joints = c.reach.join(", ");
        let into = c.at.join(", ");
        let (row, words, w_after, worse, kept_way) = if let Some(w) = kept.get(&k) {
            let (a, worse) = after.get(&k).cloned().unwrap_or_default();
            let a = sum(&a);
            let words = format!(
                "{}: its outstanding leg pointed into the flange at {into}: turned {} — kept, \
                 its joints {joints} lay out {} member end(s) on {} plate(s) (before {} on {}), \
                 none worse; written in the layout record",
                c.member,
                c.turn_words(*w),
                a.0,
                a.1,
                b.0,
                b.1
            );
            if *w == Way::Half {
                n_half += 1;
            } else {
                n_quarter += 1;
            }
            (
                FlangeTurnRow {
                    before: c.before.clone(),
                    with: c.with(*w),
                    kind: w.kind().to_owned(),
                    words: words.clone(),
                },
                words,
                a,
                worse,
                Some(*w),
            )
        } else if let Some(w) = best.get(&k).filter(|_| !c.auto) {
            let a = passing[&k]
                .iter()
                .find(|(x, _, _)| x == w)
                .map_or(b, |(_, a, _)| *a);
            let words = format!(
                "{}: its outstanding leg points into the flange at {into}: {} offered (its roll \
                 the designer's: never applied unasked) — with it its joints {joints} lay out {} \
                 member end(s) on {} plate(s) (before {} on {}), none worse; tick it in Lay out \
                 joints to apply it",
                c.member,
                c.turn_words(*w),
                a.0,
                a.1,
                b.0,
                b.1
            );
            n_offered += 1;
            (
                FlangeTurnRow {
                    before: c.before.clone(),
                    with: c.with(*w),
                    kind: OFFERED.to_owned(),
                    words: words.clone(),
                },
                words,
                a,
                Vec::new(),
                None,
            )
        } else {
            let tries: Vec<String> = tried_with
                .get(&k)
                .into_iter()
                .flatten()
                .map(|(w, (e, p), worse, blamed)| {
                    let how = match w {
                        Way::Half => "half a turn".to_owned(),
                        _ => format!("{:+.0}°", w.degrees()),
                    };
                    if !worse.is_empty() {
                        format!("{how}: joint(s) {} worse", worse.join(", "))
                    } else if *blamed {
                        format!(
                            "{how}: {e} end(s) on {p} plate(s), but a joint elsewhere laid out \
                             worse where a member turned otherwise meets its joints"
                        )
                    } else {
                        format!("{how}: {e} end(s) on {p} plate(s), no better")
                    }
                })
                .collect();
            let words = format!(
                "{}: its outstanding leg points into the flange at {into}: neither turn helps \
                 over its joints {joints} ({} end(s) on {} plate(s) as it is; {}){}; left (a \
                 stand-off, a welded assembly, later)",
                c.member,
                b.0,
                b.1,
                tries.join("; "),
                if best.contains_key(&k) {
                    " — the turns kept together did not all still help"
                } else {
                    ""
                }
            );
            n_none += 1;
            (
                FlangeTurnRow {
                    before: c.before.clone(),
                    with: c.before.clone(),
                    kind: NONE.to_owned(),
                    words: words.clone(),
                },
                words,
                b,
                Vec::new(),
                None,
            )
        };
        notes.push(words);
        weighed.push(WeighedTurn {
            run: c.member.clone(),
            kind: match kept_way.or_else(|| best.get(&k).copied()) {
                Some(Way::Half) => "half turn".to_owned(),
                Some(_) => "quarter turn".to_owned(),
                None => "flange turn".to_owned(),
            },
            members: vec![c.member.clone()],
            turn: kept_way
                .or_else(|| best.get(&k).copied())
                .map_or(0.0, Way::degrees),
            kept: kept_way.is_some(),
            joints: c.reach.clone(),
            before: b,
            after: w_after,
            worse,
        });
        rows.insert(c.member.clone(), row);
    }
    // The rows of the members not turned (offered, left): written into the
    // file before and into the run's (they change no layout).
    let others: BTreeMap<String, FlangeTurnRow> = rows
        .iter()
        .filter(|(_, r)| !r.applied())
        .map(|(m, r)| (m.clone(), r.clone()))
        .collect();
    let settled = text_with(text, &cands, &kept, &rows)?;
    let mut out = match last {
        Some(r) if !kept.is_empty() => {
            // (Every row, the kept turns' words with them, in the run's
            // record; the notes of the turns weighed before, L19.)
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
        _ => {
            let text1 = rewrite_rows(&r0.text, &others)?;
            LayoutRun {
                text: text1,
                ..r0.clone()
            }
        }
    };
    out.seconds = r0.seconds + seconds;
    let mut head = vec![format!(
        "an outstanding leg pointing into the flange (L25): {} member(s) weighed over every \
         joint they reach — half a turn kept at {n_half}, a quarter turn at {n_quarter}, offered \
         (the roll the designer's) at {n_offered}, neither helps at {n_none}; the weighing laid \
         the tower out {layouts} more time(s) ({seconds:.1} s, measured)",
        cands.len()
    )];
    head.append(&mut notes);
    out.notes.extend(head);
    out.weighed.extend(weighed);
    Ok((out, settled))
}

/// The file `text` with the record's `flange_turns` rows `rows` written
/// (each replacing the member's row there).
fn rewrite_rows(text: &str, rows: &BTreeMap<String, FlangeTurnRow>) -> Result<String, String> {
    if rows.is_empty() {
        return Ok(text.to_owned());
    }
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let mut rec = LayoutRecord::of(&t);
    for (m, r) in rows {
        rec.flange_turns.insert(m.clone(), r.clone());
    }
    rec.write(&mut doc);
    Ok(doc.to_string())
}

/// The turns the layout record of `t` offers (its roll the designer's),
/// still current: member → row.
pub fn offered(t: &StructureType) -> BTreeMap<String, FlangeTurnRow> {
    let rec = LayoutRecord::of(t);
    rec.flange_turns
        .iter()
        .filter(|(m, r)| r.kind == OFFERED && r.current(t, &rec.rolled, m))
        .map(|(m, r)| (m.clone(), r.clone()))
        .collect()
}

/// The file `text` with the offered turns of `members` applied (Lay out
/// joints' tick): each member's roll written as its row offers it, the
/// row then an applied turn ([`HALF`] or [`QUARTER`] by the turn).
pub fn apply_offered(text: &str, members: &BTreeSet<String>) -> Result<String, String> {
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let rows = offered(&t);
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let mut rec = LayoutRecord::of(&t);
    for m in members {
        let Some(r) = rows.get(m) else { continue };
        let mut d = (roll_radians(&r.with) - roll_radians(&r.before)).to_degrees() % 360.0;
        if d < 0.0 {
            d += 360.0;
        }
        let kind = if (d - 180.0).abs() < 1.0 {
            HALF
        } else {
            QUARTER
        };
        put_roll(&mut doc, m, &r.with);
        rec.rolled.remove(m);
        rec.flange_turns.insert(
            m.clone(),
            FlangeTurnRow {
                kind: kind.to_owned(),
                ..r.clone()
            },
        );
    }
    rec.write(&mut doc);
    Ok(doc.to_string())
}
