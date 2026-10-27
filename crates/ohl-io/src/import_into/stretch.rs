//! The stretch an import covers (owner, 2026-10-27): in Replace mode an
//! import holding part of the line is a variant of that part — the
//! project's objects there that the import does not hold are removed.
//!
//! - **Towers in both** are matched as the import does it: the same id
//!   within the tolerance, else the nearest of another id within it.
//! - **The import's joins**: two matched towers the import goes from one
//!   to the other with no matched tower between — along its sections
//!   (the structures of each section in order), or along its alignments
//!   (their structures in station order) when it has no section.
//! - **The project's structures in the stretch**: along each of the
//!   project's alignments (its run, in station order) and each of its
//!   sections, the structures between two matched towers that follow
//!   each other there and that the import joins. These are not in the
//!   import (they would be matched): they are removed. A line without a
//!   route still has its sections; a tower matched alone (no join) covers
//!   nothing.
//! - **The stretch's ends**: matched towers next to a structure that
//!   stays (outside the stretch) along an alignment or a section.
//! - **Sections removed**: those at a removed structure, and those over
//!   matched towers only whose every span the import joins; a section the
//!   import has at the same place (the same structures and sets) is not
//!   removed but settled as a duplicate. A section that crosses the
//!   stretch's end through a removed structure is split there: its runs
//!   of spans with an end outside the stretch stay as sections of their
//!   own ([`ohl_line::Section::piece`]; owner, 2026-10-27), the rest goes.
//! - **Jumpers removed**: those at a removed structure (or on a tap of a
//!   removed section or jumper), and those whose every end is at a
//!   matched tower inside the stretch (not one of its ends); jumpers at
//!   the ends are kept (they join what stays outside to what comes in).

use std::collections::{BTreeMap, BTreeSet};

use ohl_line::{JumperEnd, Line, Tap};

/// What the import replaces, as found (before the choices to keep).
#[derive(Debug, Default)]
pub(super) struct Found {
    /// Imported structure → the project's it is (within the tolerance).
    pub matched: BTreeMap<String, String>,
    /// The project's structures in the stretch the import covers, not in
    /// it.
    pub structures: Vec<String>,
}

/// The imported structures matched with the project's: the same id within
/// `tolerance` in plan, else the nearest of another id not matched yet
/// (imported id → the project's).
pub(super) fn matches(ours: &Line, theirs: &Line, tolerance: f64) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut used: BTreeSet<&str> = BTreeSet::new();
    for t in &theirs.structures {
        let d = |o: &ohl_line::Structure| {
            (o.position[0] - t.position[0]).hypot(o.position[1] - t.position[1])
        };
        let same = ours
            .structures
            .iter()
            .find(|o| o.id == t.id)
            .filter(|o| d(o) <= tolerance);
        let found = same.or_else(|| {
            ours.structures
                .iter()
                .filter(|o| !used.contains(o.id.as_str()) && d(o) < tolerance)
                .min_by(|a, b| d(a).total_cmp(&d(b)))
        });
        if let Some(o) = found {
            used.insert(&o.id);
            out.insert(t.id.clone(), o.id.clone());
        }
    }
    out
}

/// The structures of `line` in order: each alignment's run (station
/// order), each section's structures (`alignments`, `sections` say which).
fn sequences(line: &Line, alignments: bool, sections: bool) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    if alignments {
        out.extend(
            line.alignment_views()
                .iter()
                .map(|v| v.run().cloned().collect()),
        );
    }
    if sections {
        for s in &line.sections {
            let mut seq: Vec<String> = Vec::new();
            for p in &s.points {
                if seq.last() != Some(&p.structure) {
                    seq.push(p.structure.clone());
                }
            }
            out.push(seq);
        }
    }
    out
}

fn pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_owned(), b.to_owned())
    } else {
        (b.to_owned(), a.to_owned())
    }
}

/// The pairs of the project's towers (by the project's ids) the import
/// goes between with no other matched tower between — over its own new
/// towers too (a section ending at a new strain tower, the next going on
/// from it): along its sections, else along its alignments.
pub(super) fn joins(
    theirs: &Line,
    matched: &BTreeMap<String, String>,
) -> BTreeSet<(String, String)> {
    let by_sections = !theirs.sections.is_empty();
    // The import's structures next to each other.
    let mut next: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let seqs = sequences(theirs, !by_sections, by_sections);
    for seq in &seqs {
        for w in seq.windows(2) {
            next.entry(&w[0]).or_default().insert(&w[1]);
            next.entry(&w[1]).or_default().insert(&w[0]);
        }
    }
    let mut out = BTreeSet::new();
    for (from, m) in matched {
        // From each matched tower over new ones only, to the matched
        // towers met.
        let mut seen: BTreeSet<&str> = BTreeSet::from([from.as_str()]);
        let mut todo: Vec<&str> = vec![from.as_str()];
        while let Some(s) = todo.pop() {
            for &n in next.get(s).into_iter().flatten() {
                if !seen.insert(n) {
                    continue;
                }
                match matched.get(n) {
                    Some(o) if o != m => {
                        out.insert(pair(m, o));
                    }
                    Some(_) => {}
                    None => todo.push(n),
                }
            }
        }
    }
    out
}

/// The project's structures in the stretch: between two matched towers
/// that follow each other along one of its alignments or sections and
/// that the import joins.
pub(super) fn find(ours: &Line, theirs: &Line, tolerance: f64) -> Found {
    let matched = matches(ours, theirs, tolerance);
    let mine: BTreeSet<&String> = matched.values().collect();
    let joined = joins(theirs, &matched);
    let mut structures: Vec<String> = Vec::new();
    for seq in sequences(ours, true, true) {
        let at: Vec<usize> = (0..seq.len()).filter(|&k| mine.contains(&seq[k])).collect();
        for w in at.windows(2) {
            if joined.contains(&pair(&seq[w[0]], &seq[w[1]])) {
                for s in &seq[w[0] + 1..w[1]] {
                    if !mine.contains(s) && !structures.contains(s) {
                        structures.push(s.clone());
                    }
                }
            }
        }
    }
    Found {
        matched,
        structures,
    }
}

/// A span of the import's leaving a structure in both.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TheirSpan {
    /// The structure in both, by the project's id.
    pub from: String,
    /// Its far end, by the import's id.
    pub to: String,
    /// Where the far end stands.
    pub to_position: [f64; 3],
}

/// How far, as a share of the span's length, the far end of a project's
/// span may stand from the import's span's from the same structure for
/// the import to replace it (owner, 2026-10-27: a substation present
/// twice, 20 m apart, the old spans to its other copy left).
pub const REPLACED_SPAN_END: f64 = 0.1;

/// One thing of the project the import removes.
#[derive(Debug, Clone, PartialEq)]
pub struct Removed {
    /// Key the choice to keep it is given by: `remove:structure:x3`,
    /// `remove:section:S2`, `remove:jumper:J1`.
    pub key: String,
    /// `structure`, `section` or `jumper`.
    pub kind: &'static str,
    /// The project's id.
    pub id: String,
    /// Why, in words.
    pub why: String,
    /// Chosen to keep (it stays as the project's).
    pub keep: bool,
    /// Removed whatever is chosen: it is at a removed structure (keep the
    /// structure to keep it).
    pub fixed: bool,
    /// A section crossing the stretch's end: the runs of its points
    /// (first, last) outside the stretch, kept as sections of their own
    /// (the first under its id; owner, 2026-10-27: split, not removed
    /// whole).
    pub pieces: Vec<(usize, usize)>,
}

impl Removed {
    fn new(kind: &'static str, id: &str, why: &str, kept: &BTreeSet<String>, fixed: bool) -> Self {
        let key = format!("remove:{kind}:{id}");
        Self {
            keep: !fixed && kept.contains(&key),
            key,
            kind,
            id: id.to_owned(),
            why: why.to_owned(),
            fixed,
            pieces: Vec::new(),
        }
    }
}

/// What the import removes from the project line `ours`: the structures
/// of `found` (unless kept), the sections and jumpers as the module says.
/// `their_sections` and `their_jumpers` are the places of the import's
/// (by the project's ids): those of the project at the same place are
/// settled as duplicates, not removed. `kept` holds the keys chosen to
/// keep.
pub(super) fn removals(
    ours: &Line,
    found: &Found,
    joined: &BTreeSet<(String, String)>,
    their_sections: &BTreeSet<Vec<(String, u32)>>,
    their_jumpers: &BTreeSet<Vec<(String, u32)>>,
    their_spans: &[TheirSpan],
    kept: &BTreeSet<String>,
) -> Vec<Removed> {
    let mut out: Vec<Removed> = found
        .structures
        .iter()
        .map(|s| {
            Removed::new(
                "structure",
                s,
                "in the stretch the import covers, not in it",
                kept,
                false,
            )
        })
        .collect();
    let gone: BTreeSet<&str> = found
        .structures
        .iter()
        .filter(|s| !kept.contains(&format!("remove:structure:{s}")))
        .map(String::as_str)
        .collect();
    let mine: BTreeSet<&str> = found.matched.values().map(String::as_str).collect();
    // The stretch: matched and removed; its ends, matched towers next to
    // a structure that stays.
    let inside = |s: &str| mine.contains(s) || gone.contains(s);
    let mut ends: BTreeSet<&str> = BTreeSet::new();
    let seqs = sequences(ours, true, true);
    for seq in &seqs {
        for w in seq.windows(2) {
            for (a, b) in [(&w[0], &w[1]), (&w[1], &w[0])] {
                if mine.contains(a.as_str()) && !inside(b) {
                    ends.insert(mine.get(a.as_str()).copied().expect("a matched structure"));
                }
            }
        }
    }
    let mut sections: BTreeSet<String> = BTreeSet::new();
    // The far ends of spans the import replaces (project id → why).
    let mut replaced_ends: BTreeMap<String, String> = BTreeMap::new();
    for s in &ours.sections {
        let place: Vec<(String, u32)> = s
            .points
            .iter()
            .map(|p| (p.structure.clone(), p.set))
            .collect();
        if their_sections.contains(&place) {
            continue;
        }
        let at_gone = s
            .points
            .iter()
            .find(|p| gone.contains(p.structure.as_str()));
        let r = if let Some(p) = at_gone {
            // Its spans with an end outside the stretch, in runs: kept
            // as sections of their own (split at the stretch's end).
            let n = s.points.len();
            let outside_span = |k: usize| {
                let (a, b) = (&s.points[k].structure, &s.points[k + 1].structure);
                !gone.contains(a.as_str())
                    && !gone.contains(b.as_str())
                    && (!inside(a) || !inside(b))
            };
            let mut pieces: Vec<(usize, usize)> = Vec::new();
            let mut k = 0;
            while k + 1 < n {
                if outside_span(k) {
                    let start = k;
                    while k + 1 < n && outside_span(k) {
                        k += 1;
                    }
                    pieces.push((start, k));
                } else {
                    k += 1;
                }
            }
            let why = if pieces.is_empty() {
                format!("at the removed structure {}", p.structure)
            } else {
                format!(
                    "at the removed structure {}; it runs on outside the stretch (crosses its \
                     end): split there, the part outside kept",
                    p.structure
                )
            };
            let mut r = Removed::new("section", &s.id, &why, kept, true);
            r.pieces = pieces;
            r
        } else {
            let over_matched = s.points.iter().all(|p| mine.contains(p.structure.as_str()));
            let spans_joined = s.points.windows(2).all(|w| {
                w[0].structure == w[1].structure
                    || joined.contains(&pair(&w[0].structure, &w[1].structure))
            });
            if over_matched && spans_joined {
                Removed::new(
                    "section",
                    &s.id,
                    "in the stretch the import covers, not in it",
                    kept,
                    false,
                )
            } else if let Some((why, far)) = replaced_spans(ours, s, &mine, joined, their_spans) {
                for f in far {
                    replaced_ends.entry(f.0).or_insert(f.1);
                }
                Removed::new("section", &s.id, &why, kept, false)
            } else {
                continue;
            }
        };
        if !r.keep {
            sections.insert(s.id.clone());
        }
        out.push(r);
    }
    // Jumpers: at a removed structure, on a tap of a removed section or
    // jumper (again until none more), or with every end inside.
    let mut jumpers: BTreeSet<String> = BTreeSet::new();
    let interior = |s: &str| mine.contains(s) && !ends.contains(s);
    let mut more = true;
    while more {
        more = false;
        for j in &ours.jumpers {
            if out.iter().any(|r| r.kind == "jumper" && r.id == j.id) {
                continue;
            }
            let ends_at: Vec<(String, u32)> =
                j.points().map(|p| (p.structure.clone(), p.set)).collect();
            let mut sorted = ends_at.clone();
            sorted.sort();
            let tap_gone = [&j.from, &j.to].iter().find_map(|e| match e {
                JumperEnd::Tap {
                    tap: Tap::Span(r), ..
                } if sections.contains(&r.section) => Some(format!("section {}", r.section)),
                JumperEnd::Tap {
                    tap: Tap::Jumper { jumper, .. },
                } if jumpers.contains(jumper) => Some(format!("jumper {jumper}")),
                _ => None,
            });
            let r = if let Some(p) = j.points().find(|p| gone.contains(p.structure.as_str())) {
                Removed::new(
                    "jumper",
                    &j.id,
                    &format!("at the removed structure {}", p.structure),
                    kept,
                    true,
                )
            } else if let Some(t) = tap_gone {
                Removed::new(
                    "jumper",
                    &j.id,
                    &format!("on a tap of the removed {t}"),
                    kept,
                    true,
                )
            } else if !ends_at.is_empty()
                && ends_at.iter().all(|(s, _)| interior(s))
                && !their_jumpers.contains(&sorted)
            {
                Removed::new(
                    "jumper",
                    &j.id,
                    "inside the stretch the import covers, not in it",
                    kept,
                    false,
                )
            } else {
                continue;
            };
            if !r.keep {
                jumpers.insert(j.id.clone());
            }
            out.push(r);
            more = true;
        }
    }
    // A far end of a replaced span left with nothing (an old copy of the
    // import's structure there): removed too.
    for (x, why) in replaced_ends {
        let used = ours
            .sections
            .iter()
            .any(|s| !sections.contains(&s.id) && s.points.iter().any(|p| p.structure == x))
            || ours
                .jumpers
                .iter()
                .any(|j| !jumpers.contains(&j.id) && j.points().any(|p| p.structure == x));
        if !used {
            out.push(Removed::new("structure", &x, &why, kept, false));
        }
    }
    out
}

/// Whether every span of the project's section `s` is one the import
/// covers: joined by it, or from a structure in both to a structure the
/// import does not have but standing near the far end of one of the
/// import's spans from that structure ([`REPLACED_SPAN_END`] of the span) —
/// at least one of the latter. Why, in words, and the far ends (with
/// why, for each a structure left with nothing); none: not replaced.
fn replaced_spans(
    ours: &Line,
    s: &ohl_line::Section,
    mine: &BTreeSet<&str>,
    joined: &BTreeSet<(String, String)>,
    their_spans: &[TheirSpan],
) -> Option<(String, Vec<(String, String)>)> {
    let mut why = None;
    let mut far = Vec::new();
    for w in s.points.windows(2) {
        let (a, b) = (w[0].structure.as_str(), w[1].structure.as_str());
        if a == b || joined.contains(&pair(a, b)) {
            continue;
        }
        let (m, x) = match (mine.contains(a), mine.contains(b)) {
            (true, false) => (a, b),
            (false, true) => (b, a),
            _ => return None,
        };
        let (pm, px) = (ours.structure(m)?.position, ours.structure(x)?.position);
        let length = (px[0] - pm[0]).hypot(px[1] - pm[1]);
        let near = their_spans
            .iter()
            .filter(|t| t.from == m)
            .map(|t| {
                let d = (t.to_position[0] - px[0]).hypot(t.to_position[1] - px[1]);
                (d, t)
            })
            .filter(|(d, _)| *d <= REPLACED_SPAN_END * length)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let (d, t) = near?;
        far.push((
            x.to_owned(),
            format!(
                "left with nothing: the import's {} stands {d:.1} m from it (an older copy)",
                t.to
            ),
        ));
        why.get_or_insert_with(|| {
            format!(
                "on a span the import replaces: from {m} to {x}, which stands {d:.1} m from the \
                 import's {} at the end of its span from {m}",
                t.to
            )
        });
    }
    why.map(|w| (w, far))
}

/// The removals not kept taken out of `line`: the structures (alignment
/// points tied to them untied), sections (with their spacers) and
/// jumpers; the circuit starts at structures no longer there.
pub(super) fn remove(line: &mut Line, removed: &[Removed]) {
    let of = |kind: &str| -> BTreeSet<&str> {
        removed
            .iter()
            .filter(|r| r.kind == kind && !r.keep)
            .map(|r| r.id.as_str())
            .collect()
    };
    let (structures, sections, jumpers) = (of("structure"), of("section"), of("jumper"));
    line.structures
        .retain(|s| !structures.contains(s.id.as_str()));
    // A section crossing the stretch's end: its parts outside kept.
    let pieces: Vec<ohl_line::Section> = removed
        .iter()
        .filter(|r| r.kind == "section" && !r.keep && !r.pieces.is_empty())
        .filter_map(|r| line.sections.iter().find(|s| s.id == r.id).map(|s| (r, s)))
        .flat_map(|(r, s)| {
            let taken: BTreeSet<String> = line.sections.iter().map(|x| x.id.clone()).collect();
            r.pieces
                .iter()
                .enumerate()
                .filter_map(|(k, &(a, b))| {
                    let id = if k == 0 {
                        s.id.clone()
                    } else {
                        let mut n = k + 1;
                        while taken.contains(&format!("{}-{n}", s.id)) {
                            n += 1;
                        }
                        format!("{}-{n}", s.id)
                    };
                    s.piece(a, b, &id)
                })
                .collect::<Vec<_>>()
        })
        .collect();
    line.sections.retain(|s| !sections.contains(s.id.as_str()));
    let spared: BTreeSet<String> = pieces.iter().map(|p| p.id.clone()).collect();
    line.sections.extend(pieces);
    line.jumpers.retain(|j| !jumpers.contains(j.id.as_str()));
    // Spacers on what is gone (a spared piece's only where both ends'
    // span points are still on it).
    let on = |e: &ohl_line::SpanPointRef| {
        !sections.contains(e.section.as_str())
            || (spared.contains(&e.section)
                && line
                    .sections
                    .iter()
                    .any(|x| x.id == e.section && x.span_points.iter().any(|p| p.label == e.point)))
    };
    line.spacers.retain(|s| on(&s.a) && on(&s.b));
    for p in &mut line.alignment_points {
        if p.structure
            .as_deref()
            .is_some_and(|s| structures.contains(s))
        {
            p.structure = None;
        }
    }
    line.circuit_starts
        .retain(|p| !structures.contains(p.structure.as_str()));
}
