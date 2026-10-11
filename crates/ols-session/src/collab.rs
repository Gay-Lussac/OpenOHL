// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Collaboration: edit rights per kind, object or area, and the review of
//! branches before they are merged (ADR-0005, ADR-0010).
//!
//! **Grants.** An admin limits what a user (or every user of a role,
//! subject `role:designer`) may edit: a [`Grant`] names kinds of objects
//! (`structure`, `section`, `jumper`, `route`, `meta`, and `library` for
//! library files), objects (`T12` or `structure:T12`) and/or a plan area
//! (a polygon). A user with grants may change only what one of them
//! covers — every object a command changes, also those a deletion takes
//! along, before and after the change; a user without grants is limited
//! by their role only, and admins by nothing. The session checks every
//! command ([`Session::apply`] and undo/redo), so the server enforces the
//! same rights. Users with grants cannot check out, merge or revert (each
//! replaces the working state at large).
//!
//! **Reviews.** A designer submits a branch for review into a target
//! branch (normally `main`), naming reviewers; reviewers (or admins)
//! approve or reject it with a comment; an approved branch is merged by a
//! reviewer or an admin ([`Session::merge_review`]) — also into a
//! *protected* branch, which otherwise only admins change. A branch
//! changed after its approval must be submitted again.
//!
//! Tables (schema 2):
//!
//! ```sql
//! edit_grant(id PRIMARY KEY, subject, kinds, objects, area, user, time, note)
//! vc_review(id PRIMARY KEY, branch, target, revision, author, time, note,
//!           reviewers, state)
//! vc_review_note(seq PRIMARY KEY, review, user, time, verdict, comment)
//! ```

use std::collections::BTreeMap;

use ols_core::grid::Bounds;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::spatial::Entry;
use crate::versions::{Choices, MergeOutcome, RefKind};
use crate::{Kind, Role, Session, SessionError, refused};

/// Tables of grants and reviews.
pub(crate) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS edit_grant (
        id INTEGER PRIMARY KEY, subject TEXT NOT NULL, kinds TEXT NOT NULL,
        objects TEXT NOT NULL, area TEXT, user TEXT NOT NULL,
        time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        note TEXT NOT NULL DEFAULT '');
    CREATE TABLE IF NOT EXISTS vc_review (
        id INTEGER PRIMARY KEY, branch TEXT NOT NULL, target TEXT NOT NULL,
        revision TEXT NOT NULL, author TEXT NOT NULL,
        time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        note TEXT NOT NULL, reviewers TEXT NOT NULL, state TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS vc_review_note (
        seq INTEGER PRIMARY KEY, review INTEGER NOT NULL, user TEXT NOT NULL,
        time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        verdict TEXT NOT NULL, comment TEXT NOT NULL);";

/// Kinds a grant may name.
pub const GRANT_KINDS: [&str; 6] = ["meta", "route", "structure", "section", "jumper", "library"];

/// What a user (or a role) may edit.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    /// A user name, or `role:designer` (every user of a role).
    pub subject: String,
    /// Kinds (empty: every kind of line object; library files only when
    /// `library` is named).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<String>,
    /// Objects: `T12` or `structure:T12` (empty: any of the kinds).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<String>,
    /// Plan area, a polygon of x, y (m): an object's plan bounds must lie
    /// inside it (its corners are tested).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<Vec<[f64; 2]>>,
    /// Note.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl Grant {
    /// Whether it lets its subject change object `kind`/`id`, with plan
    /// bounds `bounds` before and after the change.
    pub fn allows(&self, kind: Kind, id: &str, bounds: &[Bounds]) -> bool {
        let kind_ok = self.kinds.is_empty() || self.kinds.iter().any(|k| k == kind.as_str());
        let object_ok = self.objects.is_empty()
            || self
                .objects
                .iter()
                .any(|o| o == id || *o == format!("{}:{id}", kind.as_str()));
        let area_ok = match &self.area {
            None => true,
            Some(poly) => {
                !bounds.is_empty()
                    && bounds.iter().all(|b| {
                        [
                            [b.min[0], b.min[1]],
                            [b.max[0], b.min[1]],
                            [b.max[0], b.max[1]],
                            [b.min[0], b.max[1]],
                        ]
                        .iter()
                        .all(|p| inside(poly, *p))
                    })
            }
        };
        kind_ok && object_ok && area_ok
    }

    /// In words: `structures, sections T1, T2 in an area of 4 points`.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        parts.push(if self.kinds.is_empty() {
            "line objects".to_owned()
        } else {
            self.kinds.join(", ")
        });
        if !self.objects.is_empty() {
            parts.push(self.objects.join(", "));
        }
        if let Some(a) = &self.area {
            parts.push(format!("in an area of {} points", a.len()));
        }
        parts.join(" ")
    }

    fn check(&self) -> Result<(), SessionError> {
        if self.subject.trim().is_empty() {
            return Err(refused("a grant needs a subject (user or role:NAME)"));
        }
        if let Some(role) = self.subject.strip_prefix("role:") {
            Role::parse(role)?;
        }
        if let Some(k) = self
            .kinds
            .iter()
            .find(|k| !GRANT_KINDS.contains(&k.as_str()))
        {
            return Err(refused(format!(
                "kind \"{k}\": one of {}",
                GRANT_KINDS.join(", ")
            )));
        }
        if let Some(a) = &self.area
            && (a.len() < 3 || a.iter().flatten().any(|v| !v.is_finite()))
        {
            return Err(refused("an area needs at least 3 finite points"));
        }
        Ok(())
    }
}

/// Whether `p` lies inside polygon `poly` (even-odd rule).
pub fn inside(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut odd = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            odd = !odd;
        }
        j = i;
    }
    odd
}

/// A grant stored.
#[derive(Debug, Clone, PartialEq)]
pub struct GrantEntry {
    /// Id (to revoke it).
    pub id: i64,
    /// The grant.
    pub grant: Grant,
    /// The admin who made it.
    pub by: String,
    /// When (UTC).
    pub time: String,
}

/// State of a review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    /// Waiting for a reviewer.
    Open,
    /// Approved: may be merged.
    Approved,
    /// Rejected: change the branch and submit again.
    Rejected,
    /// Merged into its target.
    Merged,
    /// Closed (its branch deleted).
    Closed,
}

impl ReviewState {
    /// The name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Merged => "merged",
            Self::Closed => "closed",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "approved" => Self::Approved,
            "rejected" => Self::Rejected,
            "merged" => Self::Merged,
            "closed" => Self::Closed,
            _ => Self::Open,
        }
    }
}

/// A reviewer's verdict or comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewNote {
    /// Who.
    pub user: String,
    /// When (UTC).
    pub time: String,
    /// `approve`, `reject` or `merge`.
    pub verdict: String,
    /// Comment.
    pub comment: String,
}

/// A branch submitted for review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    /// Id.
    pub id: i64,
    /// The branch.
    pub branch: String,
    /// The branch it is to be merged into.
    pub target: String,
    /// The branch's revision when submitted.
    pub revision: String,
    /// Who submitted it.
    pub author: String,
    /// When (UTC).
    pub time: String,
    /// What it is about.
    pub note: String,
    /// Who may decide (besides admins).
    pub reviewers: Vec<String>,
    /// State.
    pub state: ReviewState,
    /// Verdicts and comments, oldest first.
    pub notes: Vec<ReviewNote>,
}

impl Session {
    /// Grants of `user`: their own and their role's.
    pub fn grants_for(&self, user: &str) -> Result<Vec<Grant>, SessionError> {
        let role = self.role(user)?.map(|r| format!("role:{}", r.as_str()));
        Ok(self
            .grants()?
            .into_iter()
            .filter(|g| g.grant.subject == user || Some(&g.grant.subject) == role.as_ref())
            .map(|g| g.grant)
            .collect())
    }

    /// Every grant.
    pub fn grants(&self) -> Result<Vec<GrantEntry>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, subject, kinds, objects, area, user, time, note FROM edit_grant ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
            ))
        })?;
        let bad = |e: serde_json::Error| SessionError::Store(e.to_string());
        rows.map(|r| {
            let (id, subject, kinds, objects, area, by, time, note) = r?;
            Ok(GrantEntry {
                id,
                grant: Grant {
                    subject,
                    kinds: serde_json::from_str(&kinds).map_err(bad)?,
                    objects: serde_json::from_str(&objects).map_err(bad)?,
                    area: area
                        .map(|a| serde_json::from_str(&a))
                        .transpose()
                        .map_err(bad)?,
                    note,
                },
                by,
                time,
            })
        })
        .collect()
    }

    /// Admin: adds a grant. Returns its id.
    pub fn grant(&self, admin: &str, grant: &Grant) -> Result<i64, SessionError> {
        self.require(admin, Role::Admin)?;
        grant.check()?;
        self.conn.execute(
            "INSERT INTO edit_grant (subject, kinds, objects, area, user, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                grant.subject.trim(),
                serde_json::to_string(&grant.kinds).expect("serialises"),
                serde_json::to_string(&grant.objects).expect("serialises"),
                grant
                    .area
                    .as_ref()
                    .map(|a| serde_json::to_string(a).expect("serialises")),
                admin,
                grant.note
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Admin: removes grant `id` (the subject's edits are then limited by
    /// their other grants, or by their role only when none is left).
    pub fn revoke(&self, admin: &str, id: i64) -> Result<Grant, SessionError> {
        self.require(admin, Role::Admin)?;
        let g = self
            .grants()?
            .into_iter()
            .find(|g| g.id == id)
            .ok_or_else(|| refused(format!("no grant {id}")))?;
        self.conn
            .execute("DELETE FROM edit_grant WHERE id = ?1", params![id])?;
        Ok(g.grant)
    }

    /// Refuses users whose edits are limited by grants (admins never are).
    pub fn require_unrestricted(&self, user: &str) -> Result<(), SessionError> {
        if self.role(user)? == Some(Role::Admin) || self.grants_for(user)?.is_empty() {
            return Ok(());
        }
        Err(refused(format!(
            "{user} may edit only some objects: this changes the working state at large"
        )))
    }

    /// Refuses a change of the `changed` objects (plan bounds `before` and
    /// `after` it) that `user`'s grants do not cover.
    pub(crate) fn check_edit(
        &self,
        user: &str,
        changed: &[(Kind, String)],
        before: &BTreeMap<(Kind, String), Entry>,
        after: &BTreeMap<(Kind, String), Entry>,
        lines: Option<(&ols_line::Line, &ols_line::Line)>,
    ) -> Result<(), SessionError> {
        if self.role(user)? == Some(Role::Admin) {
            return Ok(());
        }
        let grants = self.grants_for(user)?;
        if grants.is_empty() {
            return Ok(());
        }
        for key in changed {
            // Points tied to structures the user may move follow them.
            if key.0 == Kind::Alignment
                && let Some((old, new)) = lines
                && follows_structures(old, new, &key.1, changed)
            {
                continue;
            }
            let bounds: Vec<Bounds> = [before.get(key), after.get(key)]
                .into_iter()
                .flatten()
                .map(|e| e.bounds)
                .collect();
            if !grants.iter().any(|g| g.allows(key.0, &key.1, &bounds)) {
                let may: Vec<String> = grants.iter().map(Grant::describe).collect();
                return Err(refused(format!(
                    "{user} may edit only {}: {} {} is outside",
                    may.join("; "),
                    key.0.as_str(),
                    key.1
                )));
            }
        }
        Ok(())
    }
}

/// Whether alignment `id` changed (from `old` to `new`) only by points tied
/// to structures among `changed` following them: same points, same ties,
/// only those points' positions differ.
fn follows_structures(
    old: &ols_line::Line,
    new: &ols_line::Line,
    id: &str,
    changed: &[(Kind, String)],
) -> bool {
    let (Some(a), Some(b)) = (
        old.alignments.iter().find(|a| a.id == id),
        new.alignments.iter().find(|a| a.id == id),
    ) else {
        return false;
    };
    if (&a.id, &a.name, &a.description, &a.routes, a.points.len())
        != (&b.id, &b.name, &b.description, &b.routes, b.points.len())
    {
        return false;
    }
    a.points.iter().zip(&b.points).all(|(p, q)| {
        let same =
            (&p.structure, &p.description, &p.routes) == (&q.structure, &q.description, &q.routes);
        same && (p.position == q.position
            || q.structure
                .as_ref()
                .is_some_and(|s| changed.contains(&(Kind::Structure, s.clone()))))
    })
}

impl Session {
    /// Refuses a library change by a user whose grants do not name
    /// `library`.
    pub(crate) fn check_library_edit(&self, user: &str) -> Result<(), SessionError> {
        if self.role(user)? == Some(Role::Admin) {
            return Ok(());
        }
        let grants = self.grants_for(user)?;
        if grants.is_empty()
            || grants
                .iter()
                .any(|g| g.kinds.iter().any(|k| k == "library"))
        {
            return Ok(());
        }
        Err(refused(format!(
            "{user} may not change library files (no grant names \"library\")"
        )))
    }

    /// Every review, newest first.
    pub fn reviews(&self) -> Result<Vec<Review>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, branch, target, revision, author, time, note, reviewers, state FROM vc_review ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, branch, target, revision, author, time, note, reviewers, state) = row?;
            out.push(Review {
                id,
                branch,
                target,
                revision,
                author,
                time,
                note,
                reviewers: serde_json::from_str(&reviewers)
                    .map_err(|e| SessionError::Store(e.to_string()))?,
                state: ReviewState::parse(&state),
                notes: vec![],
            });
        }
        drop(stmt);
        let mut stmt = self.conn.prepare(
            "SELECT review, user, time, verdict, comment FROM vc_review_note ORDER BY seq",
        )?;
        let notes = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                ReviewNote {
                    user: r.get(1)?,
                    time: r.get(2)?,
                    verdict: r.get(3)?,
                    comment: r.get(4)?,
                },
            ))
        })?;
        for n in notes {
            let (review, note) = n?;
            if let Some(r) = out.iter_mut().find(|r| r.id == review) {
                r.notes.push(note);
            }
        }
        Ok(out)
    }

    /// Review `id`.
    pub fn review(&self, id: i64) -> Result<Review, SessionError> {
        self.reviews()?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| refused(format!("no review #{id}")))
    }

    /// Submits branch `branch` for review into `target`, naming reviewers
    /// (who must be users of the project). Returns the review's id.
    pub fn submit_review(
        &self,
        user: &str,
        branch: &str,
        target: &str,
        reviewers: &[String],
        note: &str,
    ) -> Result<i64, SessionError> {
        self.require(user, Role::Designer)?;
        let revision = self
            .branch_revision(branch)?
            .ok_or_else(|| refused(format!("branch \"{branch}\" has no revision")))?;
        self.branch_revision(target)?;
        if branch == target {
            return Err(refused("a branch is reviewed into another branch"));
        }
        for r in reviewers {
            if self.role(r)?.is_none() {
                return Err(refused(format!("reviewer {r}: not a user of the project")));
            }
        }
        let open: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM vc_review WHERE branch = ?1 AND target = ?2 AND state IN ('open', 'approved')",
                params![branch, target],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = open {
            return Err(refused(format!(
                "review #{id} of {branch} into {target} is still open"
            )));
        }
        self.conn.execute(
            "INSERT INTO vc_review (branch, target, revision, author, note, reviewers, state) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'open')",
            params![
                branch,
                target,
                revision,
                user,
                note.trim(),
                serde_json::to_string(reviewers).expect("serialises")
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn may_review(&self, user: &str, review: &Review) -> Result<(), SessionError> {
        let admin = self.role(user)? == Some(Role::Admin);
        if admin || (review.reviewers.iter().any(|r| r == user) && review.author != user) {
            return Ok(());
        }
        Err(refused(format!(
            "{user} is not a reviewer of review #{} (reviewers: {}; admins)",
            review.id,
            if review.reviewers.is_empty() {
                "none named".to_owned()
            } else {
                review.reviewers.join(", ")
            }
        )))
    }

    /// A reviewer (or an admin) approves or rejects review `id`, with a
    /// comment.
    pub fn decide_review(
        &self,
        user: &str,
        id: i64,
        approve: bool,
        comment: &str,
    ) -> Result<ReviewState, SessionError> {
        let review = self.review(id)?;
        self.may_review(user, &review)?;
        if review.state != ReviewState::Open {
            return Err(refused(format!(
                "review #{id} is {}",
                review.state.as_str()
            )));
        }
        if self.branch_revision(&review.branch)?.as_deref() != Some(review.revision.as_str()) {
            return Err(refused(format!(
                "branch {} changed since it was submitted: submit it again",
                review.branch
            )));
        }
        let state = if approve {
            ReviewState::Approved
        } else {
            ReviewState::Rejected
        };
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE vc_review SET state = ?1 WHERE id = ?2",
            params![state.as_str(), id],
        )?;
        tx.execute(
            "INSERT INTO vc_review_note (review, user, verdict, comment) VALUES (?1, ?2, ?3, ?4)",
            params![
                id,
                user,
                if approve { "approve" } else { "reject" },
                comment.trim()
            ],
        )?;
        tx.commit()?;
        Ok(state)
    }

    /// Merges approved review `id` (by a reviewer or an admin; also into a
    /// protected branch): its target must be the current branch, and the
    /// branch as approved.
    pub fn merge_review(
        &mut self,
        user: &str,
        id: i64,
        message: Option<&str>,
        choices: &Choices,
    ) -> Result<MergeOutcome, SessionError> {
        let review = self.review(id)?;
        self.may_review(user, &review)?;
        if review.state != ReviewState::Approved {
            return Err(refused(format!(
                "review #{id} is {}, not approved",
                review.state.as_str()
            )));
        }
        if self.branch_revision(&review.branch)?.as_deref() != Some(review.revision.as_str()) {
            return Err(refused(format!(
                "branch {} changed since its approval: submit it again",
                review.branch
            )));
        }
        if !self.is_line(&review.target) {
            return Err(refused(format!(
                "project line {} (the review's target) does not exist any more",
                review.target
            )));
        }
        let message = message.map(str::to_owned).unwrap_or_else(|| {
            format!(
                "Merge {} into {} (review #{id})",
                review.branch, review.target
            )
        });
        // Merged into its target, whichever line the user works on.
        let target = review.target.clone();
        let revision = review.revision.clone();
        let outcome = self.with_line(&target, |s| {
            s.merge_as(user, &revision, Some(&message), choices, true)
        })?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE vc_review SET state = 'merged' WHERE id = ?1",
            params![id],
        )?;
        tx.execute(
            "INSERT INTO vc_review_note (review, user, verdict, comment) VALUES (?1, ?2, 'merge', '')",
            params![id, user],
        )?;
        tx.commit()?;
        Ok(outcome)
    }

    /// Whether `name` is a branch.
    pub fn is_branch(&self, name: &str) -> Result<bool, SessionError> {
        Ok(self
            .refs()?
            .iter()
            .any(|r| r.name == name && r.kind == RefKind::Branch))
    }
}
