# ADR-0010: Version control, edit rights and reviews in the project file

- **Status:** Accepted (base; refined later)
- **Date:** 2026-10-05

## Context
DESIGN_GOALS §7 asks for git-like version control of projects — revisions,
history, diff with engineering impact, branches for design alternatives,
merge, tags for issued revisions, revert — and ADR-0005 for permissions per
area or object and a review and approval workflow. ADR-0004 puts revisions,
branches and tags in the same SQLite file as the line and its operation log.
The operation log already records every change with its inverse, per user,
for undo and rollback; it is linear and shared by everyone working on the
project (one working state, ADR-0005).

## Options considered
1. **Branches as separate operation logs** — replay commands per branch.
   Exact, but a merge must replay commands on another state, where they may
   no longer apply, and old logs must be replayed to read old states.
2. **Snapshots of the whole line per revision** (the "project lines" of the
   Lines menu). Simple, but every revision costs the whole line.
3. **Content-addressed states, git style**: every object value, library
   file and tree stored once by its hash; a revision is a tree plus
   parents and a message. Merges compare states, not commands.

## Decision
Option 3, on top of the existing session.

**Storage** (`ols_session::versions`, schema 2):

- `vc_blob(hash, data)`: SHA-256 of a text → the text, zstd. The texts are
  object values (canonical JSON: keys sorted), library files, and trees.
- A **tree** lists a state's objects (`kind`, `id`, place among its kind,
  value hash) and library files (`kind`, `name`, text hash).
- `vc_revision(id, parents, author, time, message, tree, seq)`: the id is the
  SHA-256 of tree, parents, author and message — not the time, so the server,
  every replica and an offline copy compute the same id. `seq` is the last
  operation log entry when it was made.
- `vc_ref(name, kind, revision, protected, …)`: branches (named heads; `main`
  first, empty until the first commit) and tags. `ols_meta.vc_head` names the
  current branch (or a revision, detached).

A revision costs its tree (about 100 bytes per object before compression)
plus the objects that changed. Under version control: the line's objects and
the project's library files. Not: point clouds, surfaces, settings, users,
grants, reviews, the operation log.

**Working state.** The session's line and libraries belong to the current
branch; commands keep changing them through the operation log (undo per
user, admin rollback unchanged). A commit records the working state.
Checkout, merge and revert replace it as **one logged, undoable command**
(puts and deletes as computed; library files are replaced, not logged, as
all library changes); they are refused while there are uncommitted
changes, so nothing is lost silently. A checkout in server mode switches
the working state for every user.

**Diff** compares two states object by object; changed objects field by
field (paths into their JSON; arrays compared whole). The **engineering
impact** (`ols_session::impact`) solves the touched sections — changed ones
and those through changed structures, every section when a library changed
— on both sides in one weather case, with each side's own libraries, and
reports largest sag, largest tension and smallest ground clearance; bounded
by a section count and a time limit (`ols_core::budget`).

**Merge** is three-way against the nearest common ancestor (breadth-first
over the parents). Per object: equal on both sides, or changed on one side
only → taken; changed on both → merged field by field with the same rule;
fields changed differently on both sides, or a deletion against a change,
are **conflicts**, keyed `kind:id/field` (or `kind:id`, or
`library:kind/name`) and settled by choosing ours or theirs per key (an
object key settles all its fields). The merged line must validate like any
command; otherwise the merge is refused with the reason. A branch behind the
other is fast-forwarded. **Revert** is a three-way merge of the working
state with the reverted revision as base and its first parent as theirs.

**Edit rights** (`ols_session::collab`): a grant limits a user, or every user
of a role (`role:designer`), to kinds, objects and/or a plan polygon (an
object's plan bounds from the spatial index, ADR-0008, must lie inside it,
before and after the change). Users without grants keep their role's rights;
admins are never limited. Checked in the session for every command, with
everything a deletion takes along, so the server enforces it too. Users with
grants cannot check out, merge or revert.

**Reviews**: a designer submits a branch into a target branch naming
reviewers; reviewers (not the author) or admins approve or reject it with a
comment; an approved branch, unchanged since, is merged by a reviewer or an
admin. **Protected** branches are changed only by admins or through such a
review.

**Offline work**: a copy of the project (what a client joining a server
receives) has the same revision ids. A branch committed in the copy is
written to a **bundle** (its revisions and blobs, JSON), read back into the
project — ids and hashes checked — and merged like any branch.

**Server mode**: each version-control action is an `ols_server::Op`, executed
by the authority and replayed by the replicas; ids are content hashes and
reviews and grants are numbered in the server's order, so replicas stay the
same.

## Consequences
- Old project files (schema 1) are migrated when opened: the new tables are
  made, `main` has no revision until the first commit. Builds before this one
  refuse schema 2 files ("newer than supported").
- History of states, not of commands: a merge never replays commands; the
  operation log stays the record of who did what between revisions.
- Each revision stores a full tree: fine for thousands of objects; very
  large lines will want trees split by kind or tile (later).
- Not yet: cherry-pick, history export for audit, visual diff in the views,
  per-branch working states (one working state per project), live
  synchronisation of offline copies (bundles are carried by hand).

## Amendment (2026-10-06): project lines

Every **branch is a project line with a working state of its own**
(`ols_session::lines`, schema 3): `model_object`, `oplog` and
`spatial_index` carry the line's name, so several designs of the line (the
main line and its phases) are edited side by side. This replaces the shared
working state above:

- **Checkout no longer replaces anything.** It only chooses the line a
  session works on (per user on a server; the request names it). Uncommitted
  work stays on its line, so checkout is not refused for it. Detached
  checkouts are gone: a line is made from a tag or revision instead.
- **Undo and rollback are per line**: a log entry is reversed from its own
  line.
- A line made from another starts as a copy of its working state. That
  state is first recorded as a revision of the source, which becomes their
  common ancestor.
- **Bring in** merges another line into the current one. Both lines'
  uncommitted work is recorded first. It is refused, changing nothing, while
  a conflict is unsettled: conflicts are worked out on the working states
  before anything is recorded, so a server and its replicas never diverge.
- A **review merges into its target line** whichever line the reviewer
  works on.
- Lines are made by designers (theirs); renamed or deleted by their creator
  or an admin; the main line by an admin only.
- The frozen "project lines" of schema 2 (`line_snapshot`) become lines when
  a file is opened.
- Libraries, surfaces, point clouds, backgrounds and settings stay
  project-wide.

The server protocol is version 2: requests and broadcasts carry the line,
and soft locks are per line.
