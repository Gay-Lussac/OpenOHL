# Versions, branches and reviews

A project file (`.ols`) keeps its own version history, like source code in
git (DESIGN_GOALS §7, [ADR-0010](../adr/0010-version-control.md)):

- a **revision** records the line and the project's library files as they
  are, with author, time and a message ("Raised T12 by 3 m for the road
  crossing");
- **branches** hold design alternatives and what-if scenarios; `main` is the
  first;
- **tags** name revisions for good ("IFC rev B — issued for construction");
- **diff** shows what changed between two revisions, object by object and
  field by field, and its **engineering impact** (sags, tensions,
  clearances of the sections touched);
- **merge** brings a branch back, object by object and field by field;
  conflicting changes are listed and settled one by one;
- **revert** makes a revision undoing another's changes.

The operation log (`ols project log`, Edit › History) still records every
command; revisions are the named states between them.

## The working state

Every branch is a **project line** with a **working state** of its own
(see [Project lines and phases](project-lines.md)): what you edit is the
working state of the line you work on. Commands change it as before (each
logged, undoable per user and per line). A **commit** records it as a
revision of that branch. **Check out** chooses the line you work on: nothing
changes, uncommitted work stays on its line. A tag or revision is not worked
on directly: make a line from it (`branch NAME --at TAG`). In the desktop
app, the line you work on is chosen per user (Lines › Project lines); on
the command line, `checkout` also sets the line the file opens on.

In server mode every user works on their own line; a merge changes the line
it merges into, for everyone who works on or shows that line.

Not under version control: point clouds, terrain surfaces, project settings,
users and the operation log.

## On the command line

```bash
ols project status project.ols                          # branch, uncommitted changes
ols project commit project.ols --user ana -m "as imported"
ols project branch project.ols "route B" --user ben     # a new line: a copy of the current one
ols project checkout project.ols "route B" --user ben
ols project apply project.ols --user ben '{"op": "move_structure", "id": "T6", "position": [1500, 400, 505]}'
ols project diff project.ols                            # HEAD → working state, field by field
ols project diff project.ols --impact                   # … and the sections solved on both sides
ols project commit project.ols --user ben -m "T6 east of the road"
ols project history project.ols                         # revisions with branches and tags (--limit, default 50)
ols project checkout project.ols main --user ana
ols project merge project.ols "route B" --user ana --preview
ols project merge project.ols "route B" --user ana
ols project tag project.ols "IFC rev A" --user ana --note "issued for construction"
ols project revert project.ols 3f2a9c1e --user ana      # a new revision undoing 3f2a9c1e
ols project branch project.ols                          # list branches and tags
```

Revisions are named by a branch, a tag, `HEAD`, or the start of their id
(at least 4 characters).

`diff FROM TO` compares two revisions; without `TO` the working state;
without either, the head and the working state. `--brief` lists the objects
only.

### Engineering impact

`ols project diff … --impact` solves every section the change touches —
changed sections, sections through changed structures, every section when a
library file changed — on both sides, each with its own libraries, and
lists per section the largest sag, the largest tension and the smallest
vertical clearance to the ground, for example:

```text
engineering impact in "15 °C" (1 sections in 25.7ms, ground: surface DTM):
  section                  max sag, m         max tension, kN       min clearance, m
  S3              6.86 → 7.02 (+0.16)    22.15 → 21.90 (-0.25)    11.40 → 11.12 (-0.28)
```

The weather case: `--weather FILE [--case NAME]`, or still air at
`--temperature` (15 °C). The ground: `--surface NAME`, default the surface of
the checks. Wires missing in a state come from `--library` (default: the
shipped libraries). Limits: `--max-sections` (500) and `--time-limit`
(60 s); what is left is reported as not solved.

## Merging and conflicts

`merge` merges another branch into the current one, against their nearest
common ancestor:

- an object changed on one side only is taken from that side;
- an object changed on both sides is merged **field by field** (one side
  moved T6, the other turned it: both are kept);
- a field changed differently on both sides, or an object deleted on one
  side and changed on the other, is a **conflict**.

Conflicts are listed with their key, the common value and both sides; the
merge is refused until each one is settled:

```text
$ ols project merge project.ols "route B" --user ana
route B (fbf339818a): three-way merge, common ancestor d249e078b6
  conflict structure:T6/position: base [640.0,280.0,504.0], ours [1490.0,400.0,505.0], theirs [1500.0,400.0,505.0]
error: refused: 1 conflict(s) to settle (choose ours or theirs for each): structure:T6/position
$ ols project merge project.ols "route B" --user ana --choose structure:T6=theirs
```

`--choose KEY=ours|theirs` settles one conflict; an object's key
(`structure:T6`) settles all its fields; library files have keys
`library:cables/acsr.olsw`. The merged line must be valid like after any
command (a section cannot run through a structure the other side deleted);
otherwise the merge is refused with the reason. A branch with nothing new
of its own is simply moved forward (fast-forward). The merge is one logged
command in the working state and a revision with two parents.

`revert` works the same way: the changes of one revision are taken out of
the working state; changes made to the same fields since are conflicts.

## Branches and tags

`ols project branch project.ols NAME --user U [--at REV]` makes a branch;
`--protect` (admins) protects it and `--unprotect` lifts that: a **protected** branch is changed only by
admins or by merging an approved review. `--delete` lists what goes with the
branch (open reviews closed; revisions only on it stay in the file, reachable
by id) and deletes it with `--confirm`. Tags are made with `ols project tag`
and deleted by admins (`--delete --confirm`).

## Reviews

```bash
ols project branch project.ols main --user ana --protect       # admins only
ols project review project.ols --user ben --submit "route B" --into main --reviewer rita --note "T6 moved for the road"
ols project review project.ols                                  # list
ols project review project.ols --user rita --reject 1 --note "too close to the road"
ols project review project.ols --user ben --submit "route B" --reviewer rita   # changed, submitted again
ols project review project.ols --user rita --approve 2 --note "fine"
ols project review project.ols --user rita --merge 2      # into main, whatever line rita works on
```

Reviewers named in the review (not its author) and admins approve or
reject. An approved review is merged by a reviewer or an admin, also into a
protected branch; a branch changed after its approval must be submitted
again.

## Who may edit what

Admins limit what a user — or every user of a role — may edit:

```bash
ols project grant project.ols --by ana --subject ben --kind structure --kind section --area "500,-100;900,-100;900,300;500,300"
ols project grant project.ols --by ana --subject role:designer --object T12 --object T13
ols project grant project.ols                                   # list
ols project grant project.ols --by ana --revoke 2
```

A grant names kinds (`structure`, `section`, `jumper`, `route`, `meta` for
the line settings, `library` for library files), objects (`T12` or
`structure:T12`) and/or a plan area (a polygon, project coordinates). A user
with grants may change only what one of them covers — every object a command
changes, also what a deletion takes along, before and after the change (an
object's plan bounds must lie inside the area). Users without grants edit
what their role allows; admins are never limited. The session checks every
command, so a server refuses the same. Users with grants cannot merge,
bring in changes or revert, which replace the working state at large.

## Offline work

A copy of the project file (copied while it is closed, or the copy a client
joining a server receives) shares the revision ids of the original. Work offline on a
branch in the copy, commit, and carry the branch back:

```bash
ols project bundle copy.ols --export field --out field.json            # in the copy
ols project bundle project.ols --import field.json --user ben          # in the project
ols project merge project.ols field --user ana
```

The bundle holds the branch's revisions and their contents (`--as NAME` imports it under another branch name); ids and hashes
are checked when it is read. Not yet: synchronising automatically when the
copy reconnects.

## In the desktop app

**Edit › Versions…** opens the version window:

- **History**: the revisions newest first (◆ a merge), with their branches
  and tags; **Commit** with a message; branches with *Work on it* (the
  project line you edit), *protect* (admins) and *Delete…* (lists what goes
  first); per revision *Diff*, *Tag…*, *New line from it* and *Revert*.
- **Changes**: two revisions (or a revision and the working state) compared
  field by field; **Engineering impact** solves the touched sections in the
  weather case shown, on the surface of the checks.
- **Merge**: *Preview* lists what the merge does and its conflicts, each with
  *ours* / *theirs*; *Merge* when all are settled.
- **Reviews**: submit a branch, approve or reject with a comment, merge an
  approved one.
- **Rights** (admins): grants listed, revoked, made.

When a merge changes library files (shared by every line), the project is
opened again to read them.
