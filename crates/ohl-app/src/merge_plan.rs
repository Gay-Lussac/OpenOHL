// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What bringing the changes of one side into a project line would do,
//! worked out for [`crate::merge_conflicts`] (which draws it): what is
//! brought in ([`Source`]), how the two sides are called ([`Names`]), and
//! the [`Plan`] — the conflicts by object with their values in words
//! ([`crate::merge_describe`]), what the merge changes, whether the
//! result is valid, why it is refused as things are.
//!
//! [`work_out`] changes nothing. The session works the merge out
//! (`Session::merge_plan`, `Session::bring_in_plan`); where it has no
//! plan — a revert, a review into a line that is not the one worked on —
//! the same merge is put together here from the session's own parts
//! (`merge_states`, `keep_both`), and applied by the session.

use ohl_server::Op;
use ohl_session::Session;
use ohl_session::versions::{
    Change, Choices, Conflict, Diff, MergeKind, State, diff, keep_both, merge_states,
    merge_states_with, short,
};

use crate::merge_describe::{describe_field, differing, field_label, fields, value_text};
use crate::selection::Selection;

/// The most fields of one object listed (the rest are counted).
const FIELDS_SHOWN: usize = 30;

/// The most changed objects listed in the preview (the rest are counted).
const CHANGES_SHOWN: usize = 300;

/// What is brought into a line.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// Another project line as it is now, into the line worked on
    /// (Versions › Bring in changes…): both lines' uncommitted work is
    /// recorded first.
    BringIn {
        /// The line whose changes come in.
        from: String,
    },
    /// A project line, tag or revision, into the line worked on (the
    /// Merge page): refused with uncommitted work.
    Merge {
        /// Project line, tag or revision.
        other: String,
    },
    /// A review's line into its target, whichever line is worked on.
    Review {
        /// The review.
        id: i64,
    },
    /// The changes of a revision undone on the line worked on.
    Revert {
        /// The revision.
        revision: String,
    },
}

impl Source {
    /// The button that applies it.
    pub fn apply_label(&self) -> &'static str {
        match self {
            Source::BringIn { .. } => "Bring in",
            Source::Merge { .. } | Source::Review { .. } => "Merge",
            Source::Revert { .. } => "Revert",
        }
    }

    /// What the button that applies it does.
    pub fn apply_hover(&self) -> &'static str {
        match self {
            Source::BringIn { .. } => {
                "Records both lines' uncommitted work as revisions of their own, then brings the \
                 changes in as listed: a merge revision on the line you work on. The other line \
                 is not changed"
            }
            Source::Merge { .. } => {
                "Brings the changes in as listed: a merge revision on the line you work on (or \
                 the line moved forward, when it has no changes of its own)"
            }
            Source::Review { .. } => {
                "Merges the reviewed line into the review's target as listed, and closes the \
                 review"
            }
            Source::Revert { .. } => {
                "Makes a new revision that undoes the revision's changes as listed; what was \
                 changed since is kept"
            }
        }
    }

    /// The project operation applying it with `choices`, and what it is
    /// in words (`current`: the line worked on).
    pub fn op(&self, current: &str, choices: &Choices, message: Option<String>) -> (String, Op) {
        let choices = choices.clone();
        match self {
            Source::BringIn { from } => (
                format!("changes of {from} brought into {current}"),
                Op::BringIn {
                    from: from.clone(),
                    choices,
                },
            ),
            Source::Merge { other } => (
                format!("{other} merged"),
                Op::Merge {
                    other: other.clone(),
                    message,
                    choices,
                },
            ),
            Source::Review { id } => (
                format!("review #{id} merged"),
                Op::MergeReview {
                    id: *id,
                    message,
                    choices,
                },
            ),
            Source::Revert { revision } => (
                format!("revision {} reverted", short(revision)),
                Op::Revert {
                    revision: revision.clone(),
                    message,
                    choices,
                },
            ),
        }
    }
}

/// How the two sides are called in a window.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Names {
    /// The window's heading: "Bring changes of main into phase 1".
    pub heading: String,
    /// The line that changes.
    pub into: String,
    /// The choice of its side: "keep this line's".
    pub keep: String,
    /// The choice of the other side: "take main's".
    pub take: String,
    /// The columns: the common revision, this line, the other side.
    pub columns: [String; 3],
    /// The two sides in a few words ("this line", "main").
    pub sides: [String; 2],
    /// The other side when it is a project line (it can be ticked shown
    /// next to the line worked on).
    pub shown: Option<String>,
}

impl Names {
    /// `other` brought into `into` (`here`: the line worked on).
    fn lines(s: &Session, into: &str, other: &str, here: bool, suffix: &str) -> Self {
        let other_name = if s.is_line(other) || other.len() < 12 {
            other.to_owned()
        } else if other.chars().all(|c| c.is_ascii_hexdigit()) {
            format!("revision {}", short(other))
        } else {
            other.to_owned()
        };
        let this = if here {
            "this line".to_owned()
        } else {
            into.to_owned()
        };
        Self {
            heading: format!("Bring changes of {other_name} into {into}{suffix}"),
            into: into.to_owned(),
            keep: format!("keep {this}'s"),
            take: format!("take {other_name}'s"),
            columns: [
                "common".to_owned(),
                if here {
                    format!("this line ({into})")
                } else {
                    into.to_owned()
                },
                other_name.clone(),
            ],
            sides: [this, other_name],
            shown: (s.is_line(other) && other != s.current_line()).then(|| other.to_owned()),
        }
    }

    /// Revision `rev` undone on `into`.
    fn revert(into: &str, rev: &str) -> Self {
        Self {
            heading: format!("Undo revision {rev} on {into}"),
            into: into.to_owned(),
            keep: "keep this line's".to_owned(),
            take: format!("back to before {rev}"),
            columns: [
                format!("in {rev}"),
                format!("this line ({into}) now"),
                format!("before {rev}"),
            ],
            sides: ["this line".to_owned(), format!("before {rev}")],
            shown: None,
        }
    }
}

/// A field of an object in conflict: its values in words.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldRow {
    /// The key a choice for this field alone goes by (`None`: the object
    /// is settled as a whole).
    pub key: Option<String>,
    /// The field, as read.
    pub label: String,
    /// In the common revision, on this line, on the other side.
    pub texts: [String; 3],
}

/// An object both sides changed.
#[derive(Debug, Clone, PartialEq)]
pub struct ConflictObject {
    /// The key a choice goes by (`structure:T3`): it settles all its
    /// fields.
    pub key: String,
    /// What it is ("structure T3").
    pub what: String,
    /// What happened to it, in a few words.
    pub summary: String,
    /// Its fields in conflict (at most [`FIELDS_SHOWN`]).
    pub rows: Vec<FieldRow>,
    /// Fields in conflict not listed.
    pub more: usize,
    /// Both sides made it under one id: the id the other side's takes
    /// when both are kept.
    pub keep_both: Option<String>,
    /// A choice is still to be made.
    pub open: bool,
    /// What Show selects (`None`: nothing of it is drawn).
    pub target: Option<Selection>,
    /// It is in the line that changes (not deleted there).
    pub in_line: bool,
}

/// A conflict's object key and field path: `structure:T3` and `position`
/// of `structure:T3/position` (`None`: the whole object). Told with the
/// conflict's words, an id being free to hold a `/`.
fn split_key(c: &Conflict) -> (&str, Option<&str>) {
    if c.key.starts_with("library:") {
        return (&c.key, None);
    }
    for (i, _) in c.key.match_indices('/') {
        let (object, path) = (&c.key[..i], &c.key[i + 1..]);
        if c.what.strip_suffix(path).map(str::trim_end)
            == Some(object.replacen(':', " ", 1).as_str())
        {
            return (object, Some(path));
        }
    }
    (&c.key, None)
}

/// What Show selects for an object of kind `kind`.
fn target(kind: &str, id: &str) -> Option<Selection> {
    let id = id.to_owned();
    match kind {
        "structure" => Some(Selection::Structure(id)),
        "jumper" => Some(Selection::Jumper(id)),
        "spacer" => Some(Selection::Spacer(id)),
        "alignment_point" => Some(Selection::AlignmentPoint { id }),
        "section" => Some(Selection::Span {
            section: id,
            span: 0,
        }),
        _ => None,
    }
}

/// The rows of an object in conflict as a whole (deleted on one side and
/// changed on the other, made on both, or fields that do not combine),
/// and what happened in words.
fn whole_rows(kind: &str, c: &Conflict, names: &Names) -> (String, Vec<FieldRow>) {
    let [this, other] = &names.sides;
    let row = |path: &str, texts: [String; 3]| FieldRow {
        key: None,
        label: if path.is_empty() {
            "content".to_owned()
        } else {
            field_label(path)
        },
        texts,
    };
    // One side deleted it: what the other changed, field by field.
    let deleted = |kept: &serde_json::Value, ours_kept: bool| {
        let mut rows = Vec::new();
        if let Some(base) = &c.base {
            let (fb, fk) = (fields(base), fields(kept));
            for path in differing(base, kept) {
                let [common, _, changed] = describe_field(
                    kind,
                    &path,
                    fb.get(&path).copied(),
                    None,
                    fk.get(&path).copied(),
                );
                let gone = "deleted".to_owned();
                rows.push(row(
                    &path,
                    if ours_kept {
                        [common, changed, gone]
                    } else {
                        [common, gone, changed]
                    },
                ));
            }
        }
        rows
    };
    match (&c.ours, &c.theirs) {
        (Some(a), Some(b)) => {
            let fb = c.base.as_ref().map(fields).unwrap_or_default();
            let (fa, ft) = (fields(a), fields(b));
            let rows = differing(a, b)
                .iter()
                .filter(|p| p.as_str() != "id")
                .map(|p| {
                    let (b, o, t) = (fb.get(p).copied(), fa.get(p).copied(), ft.get(p).copied());
                    // Made on both sides: nothing common to compare with.
                    let texts = if c.base.is_none() {
                        let said = |v: Option<&serde_json::Value>| {
                            v.map_or("(not there)".to_owned(), |v| value_text(kind, p, v))
                        };
                        ["(not there)".to_owned(), said(o), said(t)]
                    } else {
                        describe_field(kind, p, b, o, t)
                    };
                    row(p, texts)
                })
                .collect();
            let summary = if c.base.is_none() {
                "made on both under this id".to_owned()
            } else {
                "changed on both, as a whole".to_owned()
            };
            (summary, rows)
        }
        (None, Some(t)) => (
            format!("deleted on {this}, changed on {other}"),
            deleted(t, false),
        ),
        (Some(o), None) => (
            format!("changed on {this}, deleted on {other}"),
            deleted(o, true),
        ),
        (None, None) => ("deleted on both".to_owned(), Vec::new()),
    }
}

/// The conflicts by object, their values in words (worked out with the
/// plan, read when drawn).
pub fn by_object(conflicts: &[Conflict], names: &Names) -> Vec<ConflictObject> {
    let mut out: Vec<ConflictObject> = Vec::new();
    for c in conflicts {
        let (key, path) = split_key(c);
        let (kind, id) = key.split_once(':').unwrap_or((key, ""));
        // An object's conflicts follow one another.
        let at = match out.last() {
            Some(o) if o.key == key => out.len() - 1,
            _ => match out.iter().position(|o| o.key == key) {
                Some(at) => at,
                None => {
                    out.push(ConflictObject {
                        key: key.to_owned(),
                        what: match path {
                            Some(_) => format!("{} {id}", kind.replace('_', " ")),
                            None => c.what.replace('_', " "),
                        },
                        summary: String::new(),
                        rows: Vec::new(),
                        more: 0,
                        keep_both: None,
                        open: false,
                        target: target(kind, id),
                        in_line: true,
                    });
                    out.len() - 1
                }
            },
        };
        let o = &mut out[at];
        o.open |= c.chosen.is_none();
        let rows = match path {
            Some(path) => vec![FieldRow {
                key: Some(c.key.clone()),
                label: field_label(path),
                texts: describe_field(
                    kind,
                    path,
                    c.base.as_ref(),
                    c.ours.as_ref(),
                    c.theirs.as_ref(),
                ),
            }],
            None => {
                o.keep_both = c.keep_both.clone();
                o.in_line = c.ours.is_some();
                let (summary, rows) = whole_rows(kind, c, names);
                o.summary = summary;
                rows
            }
        };
        for row in rows {
            if o.rows.len() < FIELDS_SHOWN {
                o.rows.push(row);
            } else {
                o.more += 1;
            }
        }
    }
    for o in &mut out {
        if o.summary.is_empty() {
            let labels: Vec<&str> = o.rows.iter().map(|r| r.label.as_str()).collect();
            o.summary = format!("changed on both: {}", labels.join(", "));
        }
    }
    out
}

/// What a merge changes in the line, listed.
#[derive(Debug, Clone, PartialEq)]
pub struct Changes {
    /// In a line ("objects: 2 changed, 1 added").
    pub summary: String,
    /// One row per object or library file (at most [`CHANGES_SHOWN`]).
    pub rows: Vec<String>,
    /// Rows not listed.
    pub more: usize,
}

impl Changes {
    fn of(d: &Diff) -> Self {
        let mut rows: Vec<String> = Vec::new();
        let total = d.objects.len() + d.libraries.len();
        for o in d.objects.iter().take(CHANGES_SHOWN) {
            let mut row = format!(
                "{} {} {}",
                o.change.as_str(),
                o.kind.as_str().replace('_', " "),
                o.id
            );
            if o.change == Change::Changed && !o.fields.is_empty() {
                let names: Vec<String> = o
                    .fields
                    .iter()
                    .take(4)
                    .map(|f| field_label(&f.path))
                    .collect();
                row.push_str(&format!(": {}", names.join(", ")));
                if o.fields.len() > 4 {
                    row.push_str(&format!(" and {} more", o.fields.len() - 4));
                }
            }
            rows.push(row);
        }
        for l in d.libraries.iter().take(CHANGES_SHOWN - rows.len()) {
            rows.push(format!(
                "{} library file {}/{} ({} lines out, {} in)",
                l.change.as_str(),
                l.kind,
                l.name,
                l.lines.0,
                l.lines.1
            ));
        }
        Self {
            summary: d.summary(),
            more: total - rows.len(),
            rows,
        }
    }
}

/// Why a merge cannot be made as things are, and the way out.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    /// Why.
    pub text: String,
    /// The line worked on has uncommitted work: Commit first is offered.
    pub commit_here: bool,
}

/// A merge worked out: what [`crate::merge_conflicts::panel`] shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// How the sides are called.
    pub names: Names,
    /// What the merge does, in a sentence.
    pub headline: String,
    /// The objects in conflict.
    pub objects: Vec<ConflictObject>,
    /// Objects still to settle.
    pub open: usize,
    /// What it changes in the line (`None`: not worked out before it is
    /// applied — Bring in).
    pub changes: Option<Changes>,
    /// Why the merged line would not be valid.
    pub problem: Option<String>,
    /// What to know about it.
    pub notes: Vec<String>,
    /// Why it cannot be made as things are.
    pub refusal: Option<Refusal>,
    /// There is nothing to bring in.
    pub nothing: bool,
    /// The line that changes is the one worked on (its objects can be
    /// shown in the views).
    pub here: bool,
    /// The line before (`None`: its working state, read when asked for),
    /// the line merged and their differences: for the engineering impact.
    pub(crate) before: Option<State>,
    pub(crate) merged: Option<(State, Diff)>,
}

/// A merge as the session worked it out, before it is put in words.
#[derive(Default)]
struct Raw {
    names: Names,
    headline: String,
    conflicts: Vec<Conflict>,
    before: Option<State>,
    merged: Option<(State, Diff)>,
    problem: Option<String>,
    notes: Vec<String>,
    refusal: Option<Refusal>,
    nothing: bool,
    here: bool,
}

impl Plan {
    fn of(raw: Raw) -> Self {
        let objects = by_object(&raw.conflicts, &raw.names);
        Self {
            open: objects.iter().filter(|o| o.open).count(),
            objects,
            changes: raw.merged.as_ref().map(|(_, d)| Changes::of(d)),
            names: raw.names,
            headline: raw.headline,
            problem: raw.problem,
            notes: raw.notes,
            refusal: raw.refusal,
            nothing: raw.nothing,
            here: raw.here,
            before: raw.before,
            merged: raw.merged,
        }
    }

    /// Whether it can be applied, else why not.
    pub fn ready(&self) -> Result<(), &'static str> {
        if self.nothing {
            Err("There is nothing to bring in")
        } else if self.refusal.is_some() {
            Err("Not as things are: see above")
        } else if self.open > 0 {
            Err("Choose a side for every conflict first")
        } else if self.problem.is_some() {
            Err("The line would not be valid: settle the conflicts another way")
        } else {
            Ok(())
        }
    }
}

fn text(e: ohl_session::SessionError) -> String {
    e.to_string()
}

/// The line worked on has uncommitted work: a merge of revisions is
/// refused until it is committed.
fn uncommitted(s: &Session) -> Result<Option<Refusal>, String> {
    let status = s.status().map_err(text)?;
    Ok((!status.changes.is_empty()).then(|| Refusal {
        text: format!(
            "{} has uncommitted changes ({}): commit them first. Your choices are kept.",
            s.current_line(),
            status.changes.summary()
        ),
        commit_here: true,
    }))
}

/// What a merge of revisions does, in a sentence.
fn kind_text(kind: MergeKind, names: &Names, base: Option<&str>) -> String {
    let [_, other] = &names.sides;
    match kind {
        MergeKind::UpToDate => format!(
            "Nothing to bring in: {} already contains {other}.",
            names.into
        ),
        MergeKind::FastForward => format!(
            "{} has no revision of its own since the two parted: it takes {other} as it is.",
            names.into
        ),
        MergeKind::ThreeWay => format!(
            "Both changed since their common revision{}: changes to different objects combine \
             by themselves.",
            base.map(|b| format!(" ({})", short(b))).unwrap_or_default()
        ),
    }
}

/// [`Source::BringIn`]: the session lists the conflicts and says whether
/// the result is valid; what else changes is not worked out before.
fn bring_in(s: &Session, from: &str, choices: &Choices) -> Result<Raw, String> {
    let (conflicts, problem) = s.bring_in_plan(from, choices).map_err(text)?;
    Ok(Raw {
        names: Names::lines(s, s.current_line(), from, true, ""),
        conflicts,
        problem,
        here: true,
        ..Raw::default()
    })
}

/// [`Source::Merge`], and a review into the line worked on.
fn merge(s: &Session, other: &str, names: Names, choices: &Choices) -> Result<Raw, String> {
    let plan = s.merge_plan(other, choices).map_err(text)?;
    Ok(Raw {
        headline: kind_text(plan.kind, &names, plan.base.as_deref()),
        names,
        conflicts: plan.conflicts,
        before: None,
        nothing: plan.kind == MergeKind::UpToDate,
        merged: Some((plan.merged, plan.changes)),
        problem: plan.problem,
        notes: plan.notes,
        refusal: uncommitted(s)?,
        here: true,
    })
}

/// A review into a line that is not the one worked on: the session works
/// a plan out only for the line worked on, so the same merge is worked
/// out here from its parts (the target's last revision, the review's, and
/// their common one). The merge itself is the session's.
fn merge_elsewhere(
    s: &Session,
    target: &str,
    theirs: &str,
    names: Names,
    choices: &Choices,
) -> Result<Raw, String> {
    let mut raw = Raw {
        refusal: s.has_uncommitted(target).map_err(text)?.then(|| Refusal {
            text: format!(
                "{target} has uncommitted changes: they are committed on that line first \
                 (work on {target}, then Versions › Commit…). Your choices are kept."
            ),
            commit_here: false,
        }),
        ..Raw::default()
    };
    let ours = s.branch_revision(target).map_err(text)?;
    let before = match &ours {
        Some(o) => s.revision_state(o).map_err(text)?,
        None => State::default(),
    };
    let base = match &ours {
        Some(o) => s.merge_base(o, theirs).map_err(text)?,
        None => None,
    };
    let kind = if ours.is_some() && base.as_deref() == Some(theirs) {
        MergeKind::UpToDate
    } else if base == ours {
        MergeKind::FastForward
    } else {
        MergeKind::ThreeWay
    };
    let merged = match kind {
        MergeKind::UpToDate => before.clone(),
        MergeKind::FastForward => s.revision_state(theirs).map_err(text)?,
        MergeKind::ThreeWay => {
            let base_state = match &base {
                Some(b) => s.revision_state(b).map_err(text)?,
                None => State::default(),
            };
            let theirs_state = s.revision_state(theirs).map_err(text)?;
            let (patch, kept) =
                keep_both(&base_state, &before, &theirs_state, None, choices).map_err(text)?;
            let (merged, conflicts) =
                merge_states_with(&base_state, &before, &theirs_state, &patch, choices);
            raw.conflicts = conflicts;
            raw.conflicts.extend(kept);
            merged
        }
    };
    if raw.conflicts.iter().all(|c| c.chosen.is_some()) {
        raw.problem = merged.line().err().map(|e| e.to_string());
    }
    raw.headline = kind_text(kind, &names, base.as_deref());
    raw.names = names;
    raw.nothing = kind == MergeKind::UpToDate;
    let changes = diff(&before, &merged);
    raw.merged = Some((merged, changes));
    raw.before = Some(before);
    Ok(raw)
}

/// [`Source::Review`]: into its target, whichever line is worked on.
fn review(s: &Session, id: i64, choices: &Choices) -> Result<Raw, String> {
    let rv = s.review(id).map_err(text)?;
    if !s.is_line(&rv.target) {
        return Err(format!(
            "project line {} (the review's target) does not exist any more",
            rv.target
        ));
    }
    let here = rv.target == s.current_line();
    let suffix = format!(" (review #{id})");
    let mut names = Names::lines(s, &rv.target, &rv.branch, here, &suffix);
    names.shown = names.shown.filter(|_| here);
    if here {
        merge(s, &rv.revision, names, choices)
    } else {
        merge_elsewhere(s, &rv.target, &rv.revision, names, choices)
    }
}

/// [`Source::Revert`]: the revision's changes undone on the line worked
/// on, merged with what came after it — worked out here as the session's
/// revert does (it has no plan of its own): the revision as the common
/// state, the line as it is, and the revision's parent as the other side.
fn revert(s: &Session, revision: &str, choices: &Choices) -> Result<Raw, String> {
    let id = s.resolve_revision(revision).map_err(text)?;
    let rev = s.revision(&id).map_err(text)?;
    let Some(parent) = rev.parents.first() else {
        return Err(format!(
            "revision {} is the first: there is nothing before it to go back to",
            rev.short()
        ));
    };
    let working = s.working_state().map_err(text)?;
    let (merged, conflicts) = merge_states(
        &s.revision_state(&id).map_err(text)?,
        &working,
        &s.revision_state(parent).map_err(text)?,
        choices,
    );
    let settled = conflicts.iter().all(|c| c.chosen.is_some());
    let nothing = settled && merged == working;
    let changes = diff(&working, &merged);
    Ok(Raw {
        names: Names::revert(s.current_line(), rev.short()),
        headline: if nothing {
            format!(
                "Nothing to undo: the changes of {} are not in the line (any more).",
                rev.short()
            )
        } else {
            format!(
                "A new revision undoing what {} changed (\"{}\", {}); what was changed since is \
                 kept.",
                rev.short(),
                rev.message,
                rev.author
            )
        },
        problem: settled
            .then(|| merged.line().err().map(|e| e.to_string()))
            .flatten(),
        conflicts,
        before: None,
        merged: Some((merged, changes)),
        notes: Vec::new(),
        refusal: uncommitted(s)?,
        nothing,
        here: true,
    })
}

/// The plan of `source` with `choices`; nothing is changed.
pub fn work_out(source: &Source, s: &Session, choices: &Choices) -> Result<Plan, String> {
    let raw = match source {
        Source::BringIn { from } => bring_in(s, from, choices)?,
        Source::Merge { other } => {
            let names = Names::lines(s, s.current_line(), other, true, "");
            merge(s, other, names, choices)?
        }
        Source::Review { id } => review(s, *id, choices)?,
        Source::Revert { revision } => revert(s, revision, choices)?,
    };
    Ok(Plan::of(raw))
}
