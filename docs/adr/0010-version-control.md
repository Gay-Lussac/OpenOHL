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
   Versions menu). Simple, but every revision costs the whole line.
3. **Content-addressed states, git style**: every object value, library
   file and tree stored once by its hash; a revision is a tree plus
   parents and a message. Merges compare states, not commands.

## Decision
Option 3, on top of the existing session.

**Storage** (`ohl_session::versions`, schema 2):

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
impact** (`ohl_session::impact`) solves the touched sections — changed ones
and those through changed structures, every section when a library changed
— on both sides in one weather case, with each side's own libraries, and
reports largest sag, largest tension and smallest ground clearance; bounded
by a section count and a time limit (`ohl_core::budget`).

**Merge** is three-way against a **latest common ancestor**: a common
ancestor that no other common ancestor descends from (2026-11-10; before,
the nearest by distance, which a short side line brought in could make an
old revision — values set back since were then silently overwritten and
false conflicts raised). A history that crosses has several: the nearest
of them is taken and the plan says so (`MergePlan::notes`). Per object: equal on both sides, or changed on one side
only → taken; changed on both → merged field by field with the same rule;
fields changed differently on both sides, or a deletion against a change,
are **conflicts**, keyed `kind:id/field` (or `kind:id`, or
`library:kind/name`) and settled by choosing ours or theirs per key (an
object key settles all its fields). **An object each side made under the
same id** (none in the ancestor: structure, section, jumper, spacer, route,
alignment, alignment point) can also keep **both** (2026-11-10,
`Side::Both`, `Conflict::keep_both`): theirs comes under a free id
(`T13 (2)`, the import's naming) and what refers to it on their side
follows by the rename command's rule (`rename_references`) — before, the
side not chosen was dropped and its references silently pointed at the
other object. Only the objects renamed are laid over their state
(`Patch`); no state is copied. The merged line must validate like any
command; otherwise the merge is refused with the reason. A branch behind the
other is fast-forwarded. A merge, a revert and Bring in are each **one unit
of work** (2026-11-10, `Session::unit`): the logged command, its note, the
revision (and Bring in's two recordings before it) are written inside one
savepoint, and the states the commands replace in memory are kept — moved,
not copied — and put back if a later step fails; before, a failure between
the steps left a merge looking like uncommitted changes.

More of 2026-11-10 (the version-management review), each with its test:

- **A rename against an edit.** One side renamed an object (told by its
  content: gone under the old id, one alike in all but the id added), the
  other changed it: the change is taken under the new id. Before it was
  "deleted against changed" — one choice brought the old object back
  beside the renamed one, the other lost the change. A rename *with* an
  edit of the same object on one side is not told and stays a conflict.
- **The place among a kind** (the order of alignments and their points,
  of structures) is merged as a value is: the side that moved it; both
  moved it differently: ours, without a conflict. Before, a side that
  changed only the value took its old place with it.
- **Notes left without their object** (one side deleted a structure, the
  other put a note on it) are not taken, as a deletion by a user takes
  them; the plan's notes say which.
- **A merge that is already contained** says that reverted changes come
  back by reverting the revert.
- **Bundles** are verified in full before anything is written — every
  object and library file a new revision's tree names must be in the
  bundle or the project — and blobs, revisions and the line are imported
  as one unit of work.
- `Session::ahead_behind(a, b)`: how many revisions each of two lines has
  that the other lacks (one read of the history).

- **A deletion against a use.** One side deleted an object, the other —
  which left it as it was — made or changed an object of the line that is
  on it (a section through the structure, an alignment point tied to it):
  nothing was changed on both sides, so there was no conflict, the merged
  line was not valid and the merge was refused with nothing to choose. It
  is now a conflict on the deleted object: the deletion (what the other
  side put on it is then as the deleting side has it, or not taken) or
  the object kept. References are told by name (`structure`, `section`,
  `route`/`routes`, `points`, `jumper`).

- **Library files entry by entry** (`ohl_session::library_merge`). A
  library file changed on both sides was one conflict (the base's line
  above still says "library files are compared whole" for a *diff*). Now
  the three TOML texts are compared by value, key by key: a key changed
  on one side is that side's; tables merge key by key; lists of tables
  whose entries all carry a text `name`, `id` or `label` unique in each
  text (`[[case]]`, `[[rule]]`…) merge entry by entry, an entry key by
  key; theirs' new entries come after ours. The result is ours' text with
  theirs' changes laid into it (`toml_edit`: ours' comments and layout
  stay, an entry from theirs brings its own). The same value changed
  differently, an entry removed on one side and changed on the other, a
  list without names, a text that is not TOML: one conflict, as before.
- **A structure's labelled parts part by part.** Lists inside an object
  are one value, except those whose elements are objects with a `label`
  unique in the list — a structure's attachment points, wire points and
  strings: a part changed on one side is that side's, theirs' new parts
  come after ours, a part changed differently on both sides leaves the
  list one conflict. Their order carries no meaning (they are named).
  **A section's points stay one value**: their order is the wire's way
  along the line, and a point inserted by one side has no certain place
  in the other side's order — a wrong guess would be a wrong line, so
  the choice stays the user's.

Not built yet: conflicts of one logged command settled together. **Revert** is a three-way merge of the working
state with the reverted revision as base and its first parent as theirs.

**Edit rights** (`ohl_session::collab`): a grant limits a user, or every user
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

**Server mode**: each version-control action is an `ohl_server::Op`, executed
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
(`ohl_session::lines`, schema 3): `model_object`, `oplog` and
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
- Admins **roll a line back** to any revision (`Session::roll_back_line`,
  the working tree): the line's state becomes that revision's. It is
  recorded as a new revision after the line's uncommitted work, so the
  history only grows.

The server protocol is version 2: requests and broadcasts carry the line,
and soft locks are per line.
