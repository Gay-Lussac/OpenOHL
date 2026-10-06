# Project lines and phases (the Lines menu)

A project (`.ols`) can hold several designs of the line side by side:
**project lines**. The first is the **main line**. Others are made from it,
typically one per **phase** of the project (*phase 1*, *phase 2*…) or per
alternative (*route B*). They are often shown next to the main line, for
example the line *as built* next to the line *as planned*.

Every project line is **edited on its own**: moving a structure on *phase 1*
changes *phase 1* only.

## The ideas in short

- **You work on one line at a time.** Your edits go to it, and Undo (⌘Z)
  takes back your last edit *on that line*. Choosing another line changes
  nothing; it only changes what you edit. On a server, every user chooses
  their own line, independently of the others.
- **Other lines can be shown next to it**, each in its colour, in the 3D
  view (layer *Other project lines*) and the plan. They are drawn as the line
  you work on is: structures, strings at their solved clamps, bundles and
  jumpers, in the same weather cases. When someone else edits a line you
  show, it is redrawn.
- **A new line is a copy.** It starts as the line it is made from is at that
  moment, including changes not yet committed. From then on the two are
  independent: a correction made later on the main line does **not** appear
  in the phase by itself.
- **Bringing in changes** updates a line with what changed on another since
  they parted. Changes to different objects combine by themselves. Where
  both lines changed the same object, you choose which version to keep.
- **Back into the main line through a review.** When a phase is ready, it
  is submitted for review. A reviewer or an admin approves it, then merges
  it into the main line.

What all lines share: the libraries (cables, structures, insulators, weather
cases…), the terrain surfaces, the point clouds, the backgrounds and the
project settings. What each line has of its own: the line itself
(structures, sections, jumpers, routes, line settings) and its history of
edits.

## The Lines menu

The menu stays open while you tick lines to show.

| Item | What it does |
|---|---|
| **Working on: *name*** | The line your edits go to. |
| **Project lines ›** **Project lines…** | The window listing every line: choose the one you work on and those shown; rename, delete (below). |
| **Project lines ›** **Working tree…** | The lines through time (below). |
| **New line from *name*…** | A copy of the line you work on, under a new name (e.g. *phase 1*). You keep working on your line until you choose the new one. |
| **Bring in changes from another line…** | Brings what changed on another line (by default the main line) into yours. |
| **Submit this line for review into the main line…** | When a phase is ready to go into the main line (greyed out on the main line itself). |
| ☐ *line* … | Tick to show a line next to yours, in its colour; untick to hide it. |

## The Project lines window

One row per line:

| Column | |
|---|---|
| **work on** | The line you work on (●). Click another to work on it: it is read and solved again; tick the line you left under **show** to keep seeing it. |
| **show** | Shown next to the line you work on. The line you work on is always shown. |
| **line** | Its name, in its colour when shown. Hover for who made it, when, from which line, and how many objects it holds. |
| **made by** | Who made it (*main line* for the main line). |
| **objects** | Structures, sections, jumpers, routes and settings it holds. |
| **Rename…**, **Delete…** | Greyed out when you may not (hover says why). |

Below the list: **New line from *name*…**, **Bring in changes…** and
**Submit for review…**, as in the menu.

- **Rename** a line: its history, reviews, and whoever works on it follow the
  new name.
- **Delete** a line: it goes with its uncommitted changes. Its committed
  revisions stay in the file: *Edit › Versions › New line from it* brings
  it back. Its open reviews are closed. Whoever was working on it is moved
  to another line. The line you work on and the project's only line cannot
  be deleted. Deleting is not undone with ⌘Z.

## The working tree

**Lines › Project lines › Working tree…** (also in the Project lines
window) draws the history of the project lines. The oldest is on the left
and time runs to the right: one column per revision, with the date under
the first of each day.

- **One lane per line**, the main line on top, each in its colour; *(you)*
  marks the line you work on. A line's lane starts where it was made from
  another, joined to it by a curve. Hover a line's name for who made it,
  when, and from what.
- **A dot per revision.** The larger dot is the line's latest revision; a
  ringed dot is a **merge** (changes brought in, or a review merged), joined
  to the line the changes came from. Tags are written above their revision.
- **An open dot** after a line's last revision means it has work not
  committed yet.
- **Grey, *lines deleted*:** revisions of lines that no longer exist.
- **Click a revision** for its message, author, time and line.
  - **New line from it…** starts a line from that revision.
  - **Roll back** (admins only): choose the line (by default the
    revision's own) and confirm. The line's state becomes the revision's.
    The line's uncommitted work is recorded first, and the rollback is
    recorded as a new revision, so nothing is lost and a rollback can itself
    be rolled back. Others who work on or show that line see it change.
    Designers see *Rolling a line back: admins only*.

A line whose changes were merged into another by a *fast-forward* (the other
line had not changed meanwhile) shares its revisions with it: they are drawn
on the upper lane.

### Who may do what

| | Viewer | Designer | Admin |
|---|---|---|---|
| Choose the line to work on, show lines | yes | yes | yes |
| Edit the line worked on | — | yes | yes |
| Make a new line | — | yes (it is theirs) | yes |
| Rename or delete a line | — | their own lines | every line |
| Rename or delete the **main line** | — | — | yes |
| Bring in changes | — | yes | yes |
| Submit a line for review | — | yes | yes |
| Approve and merge a review | — | if named reviewer | yes |
| Roll a line back (working tree) | — | — | yes |

Admins can also **protect** a line (Edit › Versions): it then changes only
through admins or approved reviews.

## A phase, step by step

1. On the main line, **Lines › New line from main…**, name it *phase 1*,
   **Create**.
2. **Lines › Project lines…**, click *phase 1* under **work on**. Edit it:
   replace towers, restring sections… The main line does not change. Tick
   *main* under **show** to see it next to your phase.
3. Meanwhile, a colleague corrects tower T3 on the main line. Your phase does
   not change. When you want the correction, choose **Lines › Bring in
   changes from another line…**, *changes of: main*.
   - If you did not change T3 yourself, it simply comes in: the window says
     **No conflict**. Press **Bring in**.
   - If you changed T3 too, the window lists *structure T3*. Choose **keep
     this line's** or **take main's** (or **All: …** for every object listed),
     then **Bring in**.

   It is refused, changing nothing, while an object is not settled.
4. When the phase is ready, choose **Lines › Submit this line for review
   into the main line…**, with a note. Uncommitted work is committed first.
5. A reviewer or an admin opens **Edit › Versions › Reviews**, checks the
   changes (**Changes**, with the engineering impact), **approves**, and
   **merges**. Only then does the main line change. If the phase changes
   after approval, it must be submitted again.

Starting a line, bringing in changes and submitting a review record the
lines' state as **revisions** (Edit › Versions shows them): they are the
common ground the next bring-in or merge compares against. Under the hood, project lines are the
branches of the project's version history; see
[Versions, branches and reviews](versions.md).

## On a server

Each user works on their own line, and their requests carry it.
Edits of a line reach everyone, but they only change what users who work on
that line, or show it, see. When someone renames the line you work on, you
follow it. When someone deletes it, you are moved to another line and told
so. Soft locks are per line: holding T3 on *phase 1* does not block T3 on
the main line. The web client shows the main line.

## Your choice is remembered

The line you work on and the lines you show are remembered on your computer
for each project, like the other display choices. The project opens on them
next time.

## Projects made before

Projects saved by earlier versions kept **frozen** project lines (*Create
line* saved a copy that could not be edited). They are converted when the
project is opened: each becomes an editable line of the same name and
creator. Their working state is the one saved. They have no revision in
common with the main line, so bringing changes between them compares
whole objects.

## On the command line

```bash
ols project branch project.ols "phase 1" --user ben          # a new line from the one the file opens on
ols project checkout project.ols "phase 1" --user ben        # the commands act on it from now on
ols project apply project.ols --user ben '{"op": "move_structure", "id": "T6", "position": [1500, 400, 505]}'
ols project checkout project.ols main --user ana             # back to the main line
ols project branch project.ols                               # list the lines (branches) and tags
ols project branch project.ols "phase 1" --delete --confirm --user ben
ols project history project.ols                              # the revisions with their lines and tags
```

`checkout` also sets the line the file opens on; nothing else changes.
